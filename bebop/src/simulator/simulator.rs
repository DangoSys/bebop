use super::server::socket::{CmdHandler, CmdReq, DmaReadHandler, DmaWriteHandler};
use super::sim::mode::{RunMode, SimConfig, StepMode};
use crate::buckyball::create_simulation;
use crate::buckyball::decoder::{set_cmd_handler, set_resp_tx};
use crate::buckyball::inject::inject_message;
use crate::buckyball::tdma::{set_dma_read_handler, set_dma_write_handler};
use crate::log_config::{set_backward_log, set_event_log, set_forward_log};
use serde_json;
use sim::models::model_trait::DevsModel;
use sim::simulator::Simulation;
use std::io::{self, Result, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub static CYCLE_MODE_ENABLED: AtomicBool = AtomicBool::new(false);

pub struct Simulator {
  config: SimConfig,
  cmd_handler: Arc<Mutex<CmdHandler>>,
  dma_read_handler: Arc<Mutex<DmaReadHandler>>,
  dma_write_handler: Arc<Mutex<DmaWriteHandler>>,
  cmd_rx: Receiver<CmdReq>,
  resp_tx: Sender<u64>,
  simulation: Simulation,
  global_clock: f64,
}

impl Simulator {
  pub fn new(config: SimConfig) -> Result<Self> {
    // Create separate listeners for CMD, DMA read, and DMA write
    let cmd_listener = TcpListener::bind("127.0.0.1:6000")?;
    println!("Socket server listening on 127.0.0.1:6000");
    let dma_read_listener = TcpListener::bind("127.0.0.1:6001")?;
    println!("Socket server listening on 127.0.0.1:6001 (DMA read)");
    let dma_write_listener = TcpListener::bind("127.0.0.1:6002")?;
    println!("Socket server listening on 127.0.0.1:6002 (DMA write)");
    
    // Accept connections in separate threads
    let cmd_listener_clone = cmd_listener.try_clone()?;
    let dma_read_listener_clone = dma_read_listener.try_clone()?;
    let dma_write_listener_clone = dma_write_listener.try_clone()?;
    
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (dma_read_tx, dma_read_rx) = mpsc::channel();
    let (dma_write_tx, dma_write_rx) = mpsc::channel();
    
    thread::spawn(move || {
      println!("Waiting for CMD connection on 6000...");
      match cmd_listener_clone.accept() {
        Ok((stream, addr)) => {
          println!("CMD Connected: {}", addr);
          let _ = cmd_tx.send(stream);
        }
        Err(e) => {
          eprintln!("CMD accept error: {}", e);
        }
      }
    });
    
    thread::spawn(move || {
      println!("Waiting for DMA read connection on 6001...");
      match dma_read_listener_clone.accept() {
        Ok((stream, addr)) => {
          println!("DMA read Connected: {}", addr);
          let _ = dma_read_tx.send(stream);
        }
        Err(e) => {
          eprintln!("DMA read accept error: {}", e);
        }
      }
    });
    
    thread::spawn(move || {
      println!("Waiting for DMA write connection on 6002...");
      match dma_write_listener_clone.accept() {
        Ok((stream, addr)) => {
          println!("DMA write Connected: {}", addr);
          let _ = dma_write_tx.send(stream);
        }
        Err(e) => {
          eprintln!("DMA write accept error: {}", e);
        }
      }
    });
    
    // Wait for all connections
    let cmd_stream = cmd_rx.recv().map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to receive CMD stream: {}", e)))?;
    let dma_read_stream = dma_read_rx.recv().map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to receive DMA read stream: {}", e)))?;
    let dma_write_stream = dma_write_rx.recv().map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to receive DMA write stream: {}", e)))?;
    
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
          eprintln!("Request error: {:?}", e);
          break;
        },
      }
    });

    let simulation = create_simulation();

    Ok(Self {
      config,
      cmd_handler,
      dma_read_handler,
      dma_write_handler,
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
    Ok(())
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

  fn send_response(&self, result: u64) {
    if self.resp_tx.send(result).is_err() {
      eprintln!("Failed to send response");
    }
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
    // Print messages that will be processed in this step
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
        let time0 = simulation.get_global_time();
    match simulation.step() {
      Ok(_) => {
        let until_next_event = simulation
          .models()
          .iter()
          .fold(f64::INFINITY, |min, model| f64::min(min, model.until_next_event()));
        // println!("until_next_event: {:.1}", until_next_event);
        if until_next_event == f64::INFINITY {
          // no_task_now = true;
          // println!("no task now");
          thread::sleep(Duration::from_millis(4));
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
