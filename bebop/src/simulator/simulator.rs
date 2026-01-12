use super::host::{launch_host_process, HostConfig};
use super::server::socket::{accept_connection_async, CmdHandler, CmdReq, DmaReadHandler, DmaWriteHandler};
use super::sim::mode::{SimConfig, StepMode};
use super::sim::model::model_step;
use super::sim::shell;
use super::utils::log::set_log;
use crate::buckyball::create_simulation;
use crate::buckyball::decoder::{set_cmd_handler, set_resp_tx};
use crate::buckyball::tdma_loader::set_dma_read_handler;
use crate::buckyball::tdma_storer::set_dma_write_handler;
use crate::log_info;
use crate::simulator::sim::inject::inject_message;
use serde_json;
use sim::simulator::Simulation;
use std::fs::File;
use std::io::{self, BufWriter, Result, Write};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct Simulator {
  config: SimConfig,
  cmd_rx: Receiver<CmdReq>,
  simulation: Simulation,
  global_clock: f64,
  host_process: Option<Child>,
  host_exit: Arc<AtomicBool>,
  trace_writer: Option<BufWriter<File>>,
}

impl Simulator {
  pub fn new(config: SimConfig, host_config: HostConfig) -> Result<Self> {
    // Create separate listeners for CMD, DMA read, and DMA write
    let (_cmd_listener, cmd_rx) = accept_connection_async(6000, "CMD")?;
    let (_dma_read_listener, dma_read_rx) = accept_connection_async(6001, "DMA read")?;
    let (_dma_write_listener, dma_write_rx) = accept_connection_async(6002, "DMA write")?;

    // Give the listeners a moment to be ready
    thread::sleep(Duration::from_millis(100));

    // Launch and monitor host process
    let (host_process, host_exit) = launch_host_process(host_config)?;

    let cmd_stream = cmd_rx
      .recv()
      .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to receive CMD stream: {}", e)))?;
    let dma_read_stream = dma_read_rx.recv().map_err(|e| {
      io::Error::new(
        io::ErrorKind::Other,
        format!("Failed to receive DMA read stream: {}", e),
      )
    })?;
    let dma_write_stream = dma_write_rx.recv().map_err(|e| {
      io::Error::new(
        io::ErrorKind::Other,
        format!("Failed to receive DMA write stream: {}", e),
      )
    })?;

    let cmd_handler = Arc::new(Mutex::new(CmdHandler::new(cmd_stream)));
    let dma_read_handler = Arc::new(Mutex::new(DmaReadHandler::new(dma_read_stream)));
    let dma_write_handler = Arc::new(Mutex::new(DmaWriteHandler::new(dma_write_stream)));

    set_dma_read_handler(Arc::clone(&dma_read_handler));
    set_dma_write_handler(Arc::clone(&dma_write_handler));
    set_cmd_handler(Arc::clone(&cmd_handler));

    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (resp_tx, resp_rx) = mpsc::channel();

    set_resp_tx(resp_tx.clone());

    let cmd_handler_clone = Arc::clone(&cmd_handler);

    thread::spawn(move || loop {
      let mut handler = cmd_handler_clone.lock().unwrap();
      match handler.recv_request() {
        Ok(req) => {
          if cmd_tx.send(req).is_err() {
            break;
          }
          drop(handler);
          match resp_rx.recv() {
            Ok(result) => {
              let mut handler = cmd_handler_clone.lock().unwrap();
              let _ = handler.send_response(result);
            },
            Err(_) => break,
          }
        },
        Err(e) => {
          // eprintln!("Request error: {:?}", e);
          break;
        },
      }
    });

    let simulation = create_simulation();

    // Initialize trace writer if trace file is specified
    let trace_writer = if let Some(ref path) = config.trace_file {
      let file = File::create(path)?;
      log_info!("Trace file enabled: {}", path);
      Some(BufWriter::new(file))
    } else {
      None
    };

    Ok(Self {
      config,
      cmd_rx,
      simulation,
      global_clock: 0.0,
      host_process,
      host_exit,
      trace_writer,
    })
  }

  pub fn run(&mut self) -> Result<()> {
    if self.config.quiet {
      set_log(false);
    }
    match self.config.step_mode {
      StepMode::Continuous => self.run_continuous(),
      StepMode::Step => self.run_step_mode(),
    }
  }

  fn run_step_mode(&mut self) -> Result<()> {
    log_info!("Step mode - Press Enter to step once");
    log_info!("Press Enter to continue...\n");

    loop {
      if self.host_exit.load(Ordering::Relaxed) {
        log_info!("Host process has exited, terminating bebop simulator...");
        return Ok(());
      }

      match shell::read_command()? {
        shell::Command::Step(n) => {
          for _ in 0..n {
            self.step()?;
          }
        },
        shell::Command::Quit => break,
        shell::Command::Continue => self.run_continuous()?,
      }
    }

    Ok(())
  }

  fn run_continuous(&mut self) -> Result<()> {
    loop {
      if self.host_exit.load(Ordering::Relaxed) {
        log_info!("Host process has exited, terminating bebop simulator...");
        return Ok(());
      }

      self.step()?;
    }
  }

  fn step(&mut self) -> Result<()> {
    if let Ok(req) = self.cmd_rx.try_recv() {
      let inst_json = serde_json::to_string(&vec![req.funct as u64, req.xs1, req.xs2]).unwrap();
      inject_message(&mut self.simulation, "decoder", None, None, None, &inst_json);
    }
    model_step(&mut self.simulation, &mut self.trace_writer)?;
    self.global_clock = self.simulation.get_global_time();
    Ok(())
  }
}

impl Drop for Simulator {
  fn drop(&mut self) {
    if let Some(mut child) = self.host_process.take() {
      // println!("Terminating host process...");
      let _ = child.kill();
      let _ = child.wait();
      // println!("host process terminated.");
    }
  }
}
