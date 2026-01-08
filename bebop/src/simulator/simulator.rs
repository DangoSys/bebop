use super::server::socket::{CmdHandler, CmdReq, DmaHandler};
use super::sim::mode::{RunMode, SimConfig, StepMode};
use crate::buckyball::create_simulation;
use crate::buckyball::inject::inject_message;
use crate::log_config::{set_backward_log, set_event_log, set_forward_log};
use serde_json;
use sim::models::model_trait::DevsModel;
use sim::simulator::{Message, Simulation};
use std::io::{self, Result, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

pub static CYCLE_MODE_ENABLED: AtomicBool = AtomicBool::new(false);
pub static FENCE_CSR: AtomicBool = AtomicBool::new(false);

pub struct Simulator {
  config: SimConfig,
  cmd_handler: Arc<Mutex<CmdHandler>>,
  dma_handler: Arc<Mutex<DmaHandler>>,
  cmd_rx: Receiver<CmdReq>,
  resp_tx: Sender<u64>,
  simulation: Simulation,
  global_clock: f64,
}

impl Simulator {
  pub fn new(config: SimConfig) -> Result<Self> {
    let (_, stream) = tcp_listen(9999)?;

    let cmd_handler = Arc::new(Mutex::new(CmdHandler::new(stream.try_clone()?)));
    let dma_handler = Arc::new(Mutex::new(DmaHandler::new(stream.try_clone()?)));

    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (resp_tx, resp_rx) = mpsc::channel();

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
          eprintln!("Request error: {:?}", e);
          break;
        },
      }
    });

    let simulation = create_simulation();

    Ok(Self {
      config,
      cmd_handler,
      dma_handler,
      cmd_rx,
      resp_tx,
      simulation,
      global_clock: 0.0,
    })
  }

  pub fn run(&mut self) -> Result<()> {
    if self.config.quiet {
      set_event_log(true);
      set_forward_log(true);
      set_backward_log(true);
    }
    match self.config.run_mode {
      RunMode::Func => CYCLE_MODE_ENABLED.store(false, Ordering::Relaxed),
      RunMode::Cycle => CYCLE_MODE_ENABLED.store(true, Ordering::Relaxed),
    }
    match self.config.step_mode {
      StepMode::Continuous => self.run_continuous(),
      StepMode::Step => self.run_step_mode(),
    }
  }

  fn run_step_mode(&mut self) -> Result<()> {
    println!("Step mode - Press Enter to continue, 'q' to quit, 'si<N>' to step N times");
    println!("Press Enter to continue...\n");
    loop {
      io::stdout().flush()?;
      let mut input = String::new();
      io::stdin().read_line(&mut input)?;
      let trimmed = input.trim();
      if trimmed.is_empty() {
        // Direct Enter: step once
        self.step()?;
      } else if trimmed == "q" {
        break;
      } else if trimmed.starts_with("si") {
        let num_str = trimmed[2..].trim();
        if num_str.is_empty() {
          eprintln!("Error: 'si' requires a number, e.g., 'si 100'");
          continue;
        }
        match num_str.parse::<u32>() {
          Ok(n) => {
            if n == 0 {
              eprintln!("Error: step count must be greater than 0");
              continue;
            }
            for _ in 0..n {
              self.step()?;
            }
          },
          Err(e) => {
            eprintln!("Error: invalid number '{}': {}", num_str, e);
            continue;
          },
        }
      } else {
        eprintln!(
          "Unknown command: '{}'. Use Enter to step, 'q' to quit, or 'si 100' to step N times",
          trimmed
        );
      }
    }
    Ok(())
  }

  fn run_continuous(&mut self) -> Result<()> {
    loop {
      self.step()?;
    }
  }

  fn step(&mut self) -> Result<()> {
    if let Ok(req) = self.cmd_rx.try_recv() {
      let inst: Vec<u64> = vec![req.funct as u64, req.xs1, req.xs2];
      let inst_json = serde_json::to_string(&inst).unwrap();
      inject_message(&mut self.simulation, "decoder", None, None, None, &inst_json);
      self.inst_complete(req.funct)?;
    }
    model_step(&mut self.simulation)?;
    self.global_clock = self.simulation.get_global_time();
    Ok(())
  }

  fn send_response(&self, result: u64) {
    if self.resp_tx.send(result).is_err() {
      eprintln!("Failed to send response");
    }
  }

  fn inst_complete(&self, funct: u32) -> Result<()> {
    if funct == 31 {
      FENCE_CSR.store(true, Ordering::Relaxed);
    }
    if !FENCE_CSR.load(Ordering::Relaxed) {
      self.send_response(0u64);
    }
    Ok(())
  }
}

fn tcp_listen(port: u16) -> Result<(TcpListener, TcpStream)> {
  let listener = TcpListener::bind(format!("127.0.0.1:{}", port))?;
  println!("Socket server listening on 127.0.0.1:{}", port);
  println!("Waiting for connection...");
  let (stream, addr) = listener.accept()?;
  println!("Connected: {}", addr);
  Ok((listener, stream))
}

fn model_step(simulation: &mut Simulation) -> Result<()> {
  loop {
    // println!("global_time: {:.1}", simulation.get_global_time());
    match simulation.step() {
      Ok(_) => {
        let until_next_event = simulation.models().iter().fold(f64::INFINITY, |min, model| {
          f64::min(min, model.until_next_event())
        });
        // println!("until_next_event: {:.1}", until_next_event);
        if until_next_event == f64::INFINITY {
          break;
        }
      },
      Err(e) => {
        eprintln!("Simulation step error: {:?}", e);
        return Err(io::Error::new(io::ErrorKind::Other, format!("Simulation error: {:?}", e)));
      },
    }
  }
  Ok(())
}

