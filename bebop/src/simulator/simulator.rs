use super::server::socket::{CmdHandler, CmdReq, DmaHandler};
use super::sim::mode::{RunMode, SimConfig, StepMode};
use crate::buckyball::buckyball::Buckyball;
use crate::buckyball::memdomain::tdma::DmaInterface;
use crate::log_config::{set_backward_log, set_event_log, set_forward_log};
use std::io::{self, Result, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

struct SimulatorDma {
  dma_handler: Arc<Mutex<DmaHandler>>,
}

impl DmaInterface for SimulatorDma {
  fn dma_read(&self, addr: u64, size: u32) -> Result<u64> {
    let mut handler = self.dma_handler.lock().unwrap();
    handler.read(addr, size)
  }

  fn dma_write(&self, addr: u64, data: u64, size: u32) -> Result<()> {
    let mut handler = self.dma_handler.lock().unwrap();
    handler.write(addr, data, size)
  }
}

pub static CYCLE_MODE_ENABLED: AtomicBool = AtomicBool::new(false);
pub static FENCE_CSR: AtomicBool = AtomicBool::new(false);

pub struct Simulator {
  config: SimConfig,
  cmd_handler: Arc<Mutex<CmdHandler>>,
  dma_handler: Arc<Mutex<DmaHandler>>,
  cmd_rx: Receiver<CmdReq>,
  resp_tx: Sender<u64>,
  buckyball: Buckyball,
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

    let buckyball = Buckyball::new();

    Ok(Self {
      config,
      cmd_handler,
      dma_handler,
      cmd_rx,
      resp_tx,
      buckyball,
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
        eprintln!("Unknown command: '{}'. Use Enter to step, 'q' to quit, or 'si 100' to step N times", trimmed);
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
    let dma = SimulatorDma {
      dma_handler: Arc::clone(&self.dma_handler),
    };

    if let Ok(req) = self.cmd_rx.try_recv() {
      self.buckyball.forward_step(Some((req.funct, req.xs1, req.xs2)), &dma)?;
      self.buckyball.backward_step()?;
      self.inst_complete()?;
    } else {
      self.buckyball.forward_step(None, &dma)?;
      self.buckyball.backward_step()?;
    }
    self.global_clock += 1.0;
    println!("clock: {:.1} -> {:.1}", self.global_clock - 1.0, self.global_clock);
    Ok(())
  }

  fn send_response(&self, result: u64) {
    if self.resp_tx.send(result).is_err() {
      eprintln!("Failed to send response");
    }
  }

  fn inst_complete(&self) -> Result<()> {
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
