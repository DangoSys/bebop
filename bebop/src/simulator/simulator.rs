use super::host::{launch_host, HostConfig};
use super::server::socket::{CmdHandler, CmdReq, DmaReadHandler, DmaWriteHandler};
use super::sim::mode::{SimConfig, StepMode};
use super::sim::shell;
use super::utils::log::{is_log_enabled, set_log};
use crate::log_info;
use crate::buckyball::create_simulation;
use crate::buckyball::decoder::{set_cmd_handler, set_resp_tx};
use crate::buckyball::inject::inject_message;
use crate::buckyball::tdma::{set_dma_read_handler, set_dma_write_handler};
use serde_json;
use sim::models::model_trait::DevsModel;
use sim::simulator::Simulation;
use std::io::{self, Result};
use std::net::{TcpListener, TcpStream};
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
}

impl Simulator {
  pub fn new(config: SimConfig) -> Result<Self> {
    // Create separate listeners for CMD, DMA read, and DMA write
    let (_cmd_listener, cmd_rx) = accept_connection_async(6000, "CMD")?;
    let (_dma_read_listener, dma_read_rx) = accept_connection_async(6001, "DMA read")?;
    let (_dma_write_listener, dma_write_rx) = accept_connection_async(6002, "DMA write")?;

    // Give the listeners a moment to be ready
    thread::sleep(Duration::from_millis(100));

    // Launch and monitor host process
    let (host_process, host_exit) = launch_host_simulation()?;

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

    Ok(Self {
      config,
      cmd_rx,
      simulation,
      global_clock: 0.0,
      host_process,
      host_exit,
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
    model_step(&mut self.simulation)?;
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

fn accept_connection_async(port: u16, name: &str) -> Result<(TcpListener, Receiver<TcpStream>)> {
  let listener = TcpListener::bind(format!("127.0.0.1:{}", port))?;
  // println!("Socket server listening on 127.0.0.1:{} ({})", port, name);

  let listener_clone = listener.try_clone()?;
  let (tx, rx) = mpsc::channel();
  let name_owned = name.to_string();

  thread::spawn(move || {
    // println!("Waiting for {} connection on {}...", name_owned, port);
    match listener_clone.accept() {
      Ok((stream, addr)) => {
        // println!("{} Connected: {}", name_owned, addr);
        let _ = tx.send(stream);
      },
      Err(e) => {
        eprintln!("{} accept error: {}", name_owned, e);
      },
    }
  });

  Ok((listener, rx))
}

fn launch_host_simulation() -> Result<(Option<Child>, Arc<AtomicBool>)> {
  let host_config = HostConfig::default();
  let host_exit = Arc::new(AtomicBool::new(false));

  let mut host_process = match launch_host(&host_config) {
    Ok(child) => {
      // println!("host process started with PID: {}", child.id());
      Some(child)
    },
    Err(e) => {
      eprintln!("Warning: Failed to start host process: {}", e);
      eprintln!("You may need to start host manually.");
      None
    },
  };

  // Start a thread to monitor host process
  if let Some(child) = host_process.take() {
    let exit_flag = Arc::clone(&host_exit);
    host_process = Some(child);

    // Take the child process out to move into thread
    if let Some(mut child_process) = host_process.take() {
      thread::spawn(move || match child_process.wait() {
        Ok(status) => {
          // println!("host process exited with status: {}", status);
          exit_flag.store(true, Ordering::Relaxed);
        },
        Err(e) => {
          eprintln!("Error waiting for host process: {}", e);
          exit_flag.store(true, Ordering::Relaxed);
        },
      });
    }
  }

  Ok((host_process, host_exit))
}

fn model_step(simulation: &mut Simulation) -> Result<()> {
  loop {
    if is_log_enabled() {
      let messages_to_process = simulation.get_messages();
      if !messages_to_process.is_empty() {
        for msg in messages_to_process {
          println!(
            "[MSG] t={:.1} {}:{} -> {}:{} | {}",
            msg.time(),
            msg.source_id(),
            msg.source_port(),
            msg.target_id(),
            msg.target_port(),
            msg.content()
          );
        }
      }
    }
    let time0 = simulation.get_global_time();
    match simulation.step() {
      Ok(_) => {
        let until_next_event = simulation
          .models()
          .iter()
          .fold(f64::INFINITY, |min, model| f64::min(min, model.until_next_event()));
        if until_next_event == f64::INFINITY {
          // thread::sleep(Duration::from_millis(4));
          thread::sleep(Duration::from_micros(300));
          // thread::sleep(Duration::from_nanos(1));
          break;
        }
        if until_next_event > 1.0 {
          break;
        }
        let time1 = simulation.get_global_time();
        if time1 > time0 {
          break;
        }
      },
      Err(e) => {
        eprintln!("Simulation step error: {:?}", e);
        return Err(io::Error::new(
          io::ErrorKind::Other,
          format!("Simulation error: {:?}", e),
        ));
      },
    }
  }
  Ok(())
}
