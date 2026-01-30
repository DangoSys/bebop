use super::host::host::{launch_host_process, HostConfig};
use super::server::socket::{accept_connection_async, CmdHandler, CmdReq, DmaReadHandler, DmaWriteHandler, VerilatorClient};
use super::sim::mode::{ArchType, StepMode};
use super::sim::model::model_step;
use super::sim::shell;
use crate::arch::buckyball::create_simulation;
use crate::arch::buckyball::decoder::{set_cmd_handler, set_resp_tx};
use crate::arch::buckyball::tdma_loader::set_dma_read_handler;
use crate::arch::buckyball::tdma_storer::set_dma_write_handler;
use crate::arch::gemmini::create_gemmini_simulation;
use crate::arch::gemmini::main::GemminiSimulation;
use crate::simulator::config::config::AppConfig;
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
  VerilatorRTL, // No internal simulation, just forward to Verilator
}

pub struct Simulator {
  app_config: AppConfig,
  cmd_rx: Receiver<CmdReq>,
  resp_tx: mpsc::Sender<u64>,
  simulation: SimulationType,
  global_clock: f64,
  host_process: Option<Child>,
  host_exit: Arc<AtomicBool>,
  trace_writer: Option<BufWriter<File>>,
  verilator_client: Option<Arc<Mutex<VerilatorClient>>>, // For VerilatorRTL mode
  dma_read_handler: Option<Arc<Mutex<DmaReadHandler>>>, // For VerilatorRTL DMA
  dma_write_handler: Option<Arc<Mutex<DmaWriteHandler>>>, // For VerilatorRTL DMA
  dma_thread_handle: Option<thread::JoinHandle<()>>, // DMA handling thread
  dma_stop: Arc<AtomicBool>, // Signal to stop DMA thread
}

impl Simulator {
  /// Create Simulator from AppConfig
  pub fn from_app_config(app_config: &AppConfig) -> Result<Self> {
    Self::new(app_config.clone())
  }

  pub fn new(app_config: AppConfig) -> Result<Self> {
    let arch_type = match app_config.simulation.arch_type.to_lowercase().as_str() {
      "gemmini" => ArchType::Gemmini,
      "buckyball" => ArchType::Buckyball,
      "verilator" | "verilator-rtl" => ArchType::VerilatorRTL,
      _ => {
        return Err(io::Error::new(
          io::ErrorKind::InvalidInput,
          format!("unsupported arch type: {}", app_config.simulation.arch_type),
        ));
      },
    };

    let host_config = HostConfig::from_app_config(&app_config)?;

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

    let mut simulation = match arch_type {
      ArchType::Buckyball => SimulationType::Buckyball(create_simulation()),
      ArchType::Gemmini => {
        let mut gemmini_sim = create_gemmini_simulation();
        gemmini_sim.set_dma_handlers(Arc::clone(&dma_read_handler), Arc::clone(&dma_write_handler));
        SimulationType::Gemmini(gemmini_sim)
      },
      ArchType::VerilatorRTL => SimulationType::VerilatorRTL,
    };

    // Initialize Verilator client for VerilatorRTL mode
    let verilator_client = if arch_type == ArchType::VerilatorRTL {
      info!("Connecting to Verilator RTL server...");
      Some(Arc::new(Mutex::new(VerilatorClient::connect()?)))
    } else {
      None
    };

    // Initialize trace writer if trace file is specified
    let trace_writer = if !app_config.simulation.trace_file.is_empty() {
      let file = File::create(&app_config.simulation.trace_file)?;
      info!("Trace file enabled: {}", app_config.simulation.trace_file);
      Some(BufWriter::new(file))
    } else {
      None
    };

    // Spawn DMA handling thread for VerilatorRTL mode
    let dma_stop = Arc::new(AtomicBool::new(false));
    let dma_thread_handle = if arch_type == ArchType::VerilatorRTL {
      let verilator_client = verilator_client.as_ref().map(|c| Arc::clone(c));
      let dma_read_handler = dma_read_handler.clone();
      let dma_write_handler = dma_write_handler.clone();
      let dma_stop = Arc::clone(&dma_stop);

      Some(thread::spawn(move || {
        eprintln!("[Bebop DMA Thread] Started");
        loop {
          if dma_stop.load(Ordering::Relaxed) {
            break;
          }

          if let Some(ref client) = verilator_client {
            let mut client = client.lock().unwrap();

            // Try to receive DMA read request (blocking with timeout would be ideal)
            eprintln!("[Bebop DMA Thread] Waiting for DMA read request...");
            match client.recv_dma_read_request() {
              Ok(dma_req) => {
                let addr = dma_req.addr;
                let size = dma_req.size;
                eprintln!("[Bebop DMA] Received read request: addr=0x{:x}, size={}", addr, size);
                let mut h = dma_read_handler.lock().unwrap();
                eprintln!("[Bebop DMA] Locked dma_read_handler, calling Spike read");
                match h.read(addr, size) {
                  Ok(data) => {
                    let data_lo = data as u64;
                    let data_hi = (data >> 64) as u64;
                    eprintln!("[Bebop DMA] Read data from Spike: lo=0x{:x}, hi=0x{:x}", data_lo, data_hi);
                    match client.send_dma_read_response(data_lo, data_hi) {
                      Ok(_) => {
                        eprintln!("[Bebop DMA] Sent read response");
                      }
                      Err(e) => {
                        eprintln!("[Bebop DMA] Failed to send read response: {}", e);
                      }
                    }
                  }
                  Err(e) => {
                    eprintln!("[Bebop DMA] DMA read from Spike failed: {}", e);
                  }
                }
              }
              Err(e) => {
                eprintln!("[Bebop DMA] recv_dma_read_request error: {}", e);
                break;
              }
            }

            drop(client);
          } else {
            break;
          }
        }
        eprintln!("[Bebop DMA Thread] Exiting");
      }))
    } else {
      None
    };

    Ok(Self {
      app_config,
      cmd_rx,
      resp_tx,
      simulation,
      global_clock: 0.0,
      host_process,
      host_exit,
      trace_writer,
      verilator_client,
      dma_read_handler: if arch_type == ArchType::VerilatorRTL { Some(dma_read_handler) } else { None },
      dma_write_handler: if arch_type == ArchType::VerilatorRTL { Some(dma_write_handler) } else { None },
      dma_thread_handle,
      dma_stop,
    })
  }

  pub fn run(&mut self) -> Result<()> {
    set_log(!self.app_config.simulation.quiet);
    let step_mode = if self.app_config.simulation.step_mode {
      StepMode::Step
    } else {
      StepMode::Continuous
    };
    match step_mode {
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
          let result = gemmini_sim.execute(req.funct as u64, req.xs1, req.xs2);
          let _ = self.resp_tx.send(result);
        },
        SimulationType::VerilatorRTL => {
          // Forward CMD to Verilator
          if let Some(ref client) = self.verilator_client {
            let mut client = client.lock().unwrap();
            match client.send_cmd(req.funct, req.xs1, req.xs2) {
              Ok(result) => {
                let _ = self.resp_tx.send(result);
              }
              Err(e) => {
                eprintln!("Failed to send CMD to Verilator: {}", e);
              }
            }
          }
        },
      }
    }

    match &mut self.simulation {
      SimulationType::Buckyball(sim) => {
        model_step(sim, &mut self.trace_writer)?;
        self.global_clock = sim.get_global_time();
      },
      SimulationType::Gemmini(_) => {
        self.global_clock += 1.0;
      },
      SimulationType::VerilatorRTL => {
        // Verilator handles its own time stepping
        self.global_clock += 1.0;
      },
    }

    Ok(())
  }
}

impl Drop for Simulator {
  fn drop(&mut self) {
    // Signal DMA thread to stop
    self.dma_stop.store(true, Ordering::Relaxed);

    // Wait for DMA thread to finish
    if let Some(handle) = self.dma_thread_handle.take() {
      let _ = handle.join();
    }

    if let Some(mut child) = self.host_process.take() {
      let _ = child.kill();
      let _ = child.wait();
    }
  }
}
