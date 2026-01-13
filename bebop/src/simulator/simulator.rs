use super::host::host::{launch_host_process, HostConfig};
use super::server::socket::{accept_connection_async, CmdHandler, CmdReq, DmaReadHandler, DmaWriteHandler};
use super::sim::mode::{ArchType, SimConfig, StepMode};
use super::sim::model::model_step;
use super::sim::shell;
use crate::arch::buckyball::create_simulation;
use crate::arch::buckyball::decoder::{set_cmd_handler, set_resp_tx};
use crate::arch::buckyball::tdma_loader::set_dma_read_handler;
use crate::arch::buckyball::tdma_storer::set_dma_write_handler;
use crate::arch::gemmini::create_gemmini_simulation;
use crate::arch::gemmini::main::GemminiSimulation;
use crate::simulator::sim::inject::inject_message;
use crate::simulator::utils::log::set_log;
use log::info;
use serde_json;
use sim::simulator::Simulation;
use std::fs::File;
use std::io::{self, BufWriter, Result};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

enum SimulationType {
  Buckyball(Simulation),
  Gemmini(GemminiSimulation),
}

pub struct Simulator {
  config: SimConfig,
  cmd_rx: Receiver<CmdReq>,
  resp_tx: mpsc::Sender<u64>,
  simulation: SimulationType,
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

    let mut simulation = match config.arch_type {
      ArchType::Buckyball => SimulationType::Buckyball(create_simulation()),
      ArchType::Gemmini => {
        let mut gemmini_sim = create_gemmini_simulation();
        gemmini_sim.set_dma_handlers(Arc::clone(&dma_read_handler), Arc::clone(&dma_write_handler));
        SimulationType::Gemmini(gemmini_sim)
      },
    };

    // Initialize trace writer if trace file is specified
    let trace_writer = if let Some(ref path) = config.trace_file {
      let file = File::create(path)?;
      info!("Trace file enabled: {}", path);
      Some(BufWriter::new(file))
    } else {
      None
    };

    Ok(Self {
      config,
      cmd_rx,
      resp_tx,
      simulation,
      global_clock: 0.0,
      host_process,
      host_exit,
      trace_writer,
    })
  }

  pub fn run(&mut self) -> Result<()> {
    set_log(!self.config.quiet);
    match self.config.step_mode {
      StepMode::Continuous => self.run_continuous(),
      StepMode::Step => self.run_step_mode(),
    }
  }

  fn run_step_mode(&mut self) -> Result<()> {
    info!("Step mode - Press Enter to step once");
    info!("Press Enter to continue...\n");

    loop {
      if self.host_exit.load(Ordering::Relaxed) {
        info!("\nHost process has exited, terminating bebop simulator...");
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
        info!("\nHost process has exited, terminating bebop simulator...");
        return Ok(());
      }

      self.step()?;
    }
  }

  fn step(&mut self) -> Result<()> {
    if let Ok(req) = self.cmd_rx.try_recv() {
      match &mut self.simulation {
        SimulationType::Buckyball(sim) => {
          let inst_json = serde_json::to_string(&vec![req.funct as u64, req.xs1, req.xs2]).unwrap();
          inject_message(sim, "decoder", None, None, None, &inst_json);
        },
        SimulationType::Gemmini(gemmini_sim) => {
          // For Gemmini, directly execute the instruction
          let result = gemmini_sim.execute(req.funct as u64, req.xs1, req.xs2);
          // Send response back immediately for functional simulation
          let _ = self.resp_tx.send(result);
        },
      }
    }

    match &mut self.simulation {
      SimulationType::Buckyball(sim) => {
        model_step(sim, &mut self.trace_writer)?;
        self.global_clock = sim.get_global_time();
      },
      SimulationType::Gemmini(_) => {
        // Gemmini is functional simulation, no cycle-accurate stepping needed
        self.global_clock += 1.0;
      },
    }

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
