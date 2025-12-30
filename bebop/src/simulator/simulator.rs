use super::server::socket::{CmdHandler, CmdReq, DmaHandler};
use super::sim::mode::{SimConfig, SimMode};
use super::utils::report::print_simulation_records;
use crate::buckyball::buckyball::Buckyball;
use crate::buckyball::frontend::bundles::rocc_frontend::RoccInstruction;
use crate::log_config::{set_backward_log, set_event_log, set_forward_log};
use sim::models::Model;
use sim::simulator::{Message, Simulation};
use std::io::{self, Result, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct Simulator {
  simulation: Simulation,
  config: SimConfig,
  _cmd_handler: Arc<Mutex<CmdHandler>>,
  _dma_handler: Arc<Mutex<DmaHandler>>,
  cmd_rx: Receiver<CmdReq>,
  resp_tx: Sender<u64>,
  pending_request: Option<CmdReq>,
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
    let models = vec![Model::new("buckyball".to_string(), Box::new(buckyball))];

    let connectors = vec![];

    let simulation = Simulation::post(models, connectors);

    // 启动后台线程处理socket请求/响应
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

            // 发送到主线程
            if cmd_tx.send(req).is_err() {
              break;
            }

            // 等待响应
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
      simulation,
      config,
      _cmd_handler: cmd_handler,
      _dma_handler: dma_handler,
      cmd_rx,
      resp_tx,
      pending_request: None,
    })
  }

  pub fn run(&mut self) -> Result<()> {
    if self.config.enable_log {
      set_event_log(true);
      set_forward_log(true);
      set_backward_log(true);
    }
    match self.config.mode {
      SimMode::Step => self.run_step_mode(),
      SimMode::Run => self.run_continuous(),
    }
  }

  fn run_step_mode(&mut self) -> Result<()> {
    println!("Step mode - Press Enter to continue, 'q' to quit\n");
    loop {
      print!("Press Enter to continue...");
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
    // 检查是否有新的请求
    if self.pending_request.is_none() {
      if let Ok(req) = self.cmd_rx.try_recv() {
        let funct = req.funct;
        let xs1 = req.xs1;
        let xs2 = req.xs2;
        println!("\n=== New request: funct={} ===", funct);

        // 创建 RoccInstruction 并序列化为 JSON
        let rocc_inst = RoccInstruction::new(funct, xs1, xs2);
        let content = serde_json::to_string(&rocc_inst).expect("Failed to serialize RoccInstruction");
        let msg = Message::new(
          "external".to_string(),
          "external".to_string(),
          "buckyball".to_string(),
          "inject".to_string(),
          self.simulation.get_global_time(),
          content,
        );
        self.simulation.inject_input(msg);

        self.pending_request = Some(req);
      }
    }

    // 执行一步仿真
    let time_before = self.simulation.get_global_time();
    self
      .simulation
      .step()
      .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{:?}", e)))?;
    let time_after = self.simulation.get_global_time();

    // 打印时间
    println!("Time: {:.1} -> {:.1}", time_before, time_after);

    // 检查是否完成
    if self.simulation.get_global_time() == f64::INFINITY && self.pending_request.is_some() {
      println!("=== Request completed ===");

      // 如果启用log，打印records
      if self.config.enable_log {
        print_simulation_records(&mut self.simulation);
      }

      let _ = self.resp_tx.send(0);
      self.pending_request = None;
    }

    Ok(())
  }
}
