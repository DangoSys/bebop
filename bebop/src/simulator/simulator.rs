use super::server::socket::{CmdHandler, CmdReq, DmaHandler};
use super::sim::mode::{RunMode, SimConfig, StepMode};
use crate::buckyball::buckyball::Buckyball;
use crate::log_config::{set_backward_log, set_event_log, set_forward_log};
use std::io::{self, Result, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

/// 全局周期模式标志
pub static CYCLE_MODE_ENABLED: AtomicBool = AtomicBool::new(false);

pub struct Simulator {
  config: SimConfig,
  _cmd_handler: Arc<Mutex<CmdHandler>>,
  _dma_handler: Arc<Mutex<DmaHandler>>,
  cmd_rx: Receiver<CmdReq>,
  resp_tx: Sender<u64>,
  buckyball: Buckyball,
}

impl Simulator {
  pub fn new(config: SimConfig) -> Result<Self> {
    let listener = TcpListener::bind("127.0.0.1:9999")?;
    println!("Socket server listening on 127.0.0.1:9999");

    println!("Waiting for Spike connection...");
    let (stream, addr) = listener.accept()?;
    println!("Connected: {}", addr);

    let cmd_handler = Arc::new(Mutex::new(CmdHandler::new(stream.try_clone()?)));
    let dma_handler = Arc::new(Mutex::new(DmaHandler::new(stream.try_clone()?)));

    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (resp_tx, resp_rx) = mpsc::channel();

    let buckyball = Buckyball::new();

    let cmd_handler_clone = Arc::clone(&cmd_handler);
    thread::spawn(move || {
      loop {
        // 接收请求
        let mut handler = cmd_handler_clone.lock().unwrap();
        match handler.recv_request() {
          Ok(req) => {
            let funct = req.funct;
            let xs1 = req.xs1;
            let xs2 = req.xs2;
            println!("Received request: funct={}, xs1={:#x}, xs2={:#x}", funct, xs1, xs2);

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
      }
    });

    Ok(Self {
      config,
      _cmd_handler: cmd_handler,
      _dma_handler: dma_handler,
      cmd_rx,
      resp_tx,
      buckyball,
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
    println!("Step mode - Press Enter to continue, 'q' to quit");
    println!("Press Enter to continue...\n");
    loop {
      io::stdout().flush()?;
      let mut input = String::new();
      io::stdin().read_line(&mut input)?;
      if input.trim() == "q" {
        break;
      }
      self.step()?;
    }
    Ok(())
  }

  fn run_continuous(&mut self) -> Result<()> {
    println!("Continuous mode\n");
    loop {
      self.step()?;
    }
  }

  fn step(&mut self) -> Result<()> {
    if let Ok(req) = self.cmd_rx.try_recv() {
      self.buckyball.inst_execute(req.funct, req.xs1, req.xs2);
    }

    if CYCLE_MODE_ENABLED.load(Ordering::Relaxed) {
      let responses = self.buckyball.cycle_advance()?;
      if !responses.is_empty() {
        println!("Received {} response(s): {:?}", responses.len(), responses);
      }
    }

    Ok(())
  }
}
