use crate::simulator::sim::mode::SimMode;
use sim::models::Model;
use sim::simulator::{Connector, Simulation, Message};
use sim::utils::errors::SimulationError;
use std::io::{self, Write};

use super::frontend::frontend::Frontend;
use super::balldomain::compute::Compute;
use super::memdomain::memory::Memory;

pub struct Buckyball {
  simulation: Simulation,
  mode: SimMode,
  step_count: usize,
}

impl Buckyball {
  pub fn new(mode: SimMode) -> Self {
    let models = vec![
      Model::new(
        String::from("frontend"),
        Box::new(Frontend::new()),
      ),
      Model::new(
        String::from("compute"),
        Box::new(Compute::new()),
      ),
      Model::new(
        String::from("memory"),
        Box::new(Memory::new()),
      ),
    ];

    // 定义模块间的连接
    // Frontend (Decoder+ROB) -> Compute <-> Memory
    let connectors = vec![
      Connector::new(
        String::from("frontend_to_compute"),
        String::from("frontend"),
        String::from("compute"),
        String::from("to_compute"),
        String::from("from_frontend"),
      ),
      Connector::new(
        String::from("compute_to_memory"),
        String::from("compute"),
        String::from("memory"),
        String::from("mem_request"),
        String::from("request"),
      ),
      Connector::new(
        String::from("memory_to_compute"),
        String::from("memory"),
        String::from("compute"),
        String::from("response"),
        String::from("mem_response"),
      ),
    ];

    let simulation = Simulation::post(models, connectors);

    Self {
      simulation,
      mode,
      step_count: 0,
    }
  }

  pub fn inject_instruction(&mut self, instruction: &str) {
    let msg = Message::new(
      String::from("external"),
      String::from("external"),
      String::from("frontend"),
      String::from("instruction"),
      self.simulation.get_global_time(),
      instruction.to_string(),
    );
    self.simulation.inject_input(msg);
    println!(">>> 注入指令: {}", instruction);
  }

  pub fn run(&mut self) -> io::Result<()> {
    println!("=== NPU模拟器启动 ===");
    println!("模式: {:?}\n", self.mode);

    // 注入一条测试指令
    self.inject_instruction("COMPUTE_TASK_001");

    match self.mode {
      SimMode::Step => self.run_step_mode(),
      SimMode::Run => self.run_continuous_mode(),
    }
  }

  fn run_step_mode(&mut self) -> io::Result<()> {
    println!("单步模式 - 按回车执行下一步，输入 'q' 退出\n");

    loop {
      print!("按回车继续...");
      io::stdout().flush()?;

      let mut input = String::new();
      io::stdin().read_line(&mut input)?;

      if input.trim() == "q" {
        break;
      }

      match self.execute_step() {
        Ok(true) => continue,
        Ok(false) => {
          println!("\n模拟完成 - 无更多事件");
          break;
        }
        Err(e) => {
          eprintln!("错误: {:?}", e);
          break;
        }
      }
    }

    self.print_summary();
    Ok(())
  }

  fn run_continuous_mode(&mut self) -> io::Result<()> {
    println!("连续运行模式\n");

    loop {
      match self.execute_step() {
        Ok(true) => continue,
        Ok(false) => {
          println!("\n模拟完成 - 无更多事件");
          break;
        }
        Err(e) => {
          eprintln!("错误: {:?}", e);
          break;
        }
      }
    }

    self.print_summary();
    Ok(())
  }

  fn execute_step(&mut self) -> Result<bool, SimulationError> {
    let messages = self.simulation.step()?;
    let current_time = self.simulation.get_global_time();

    // 打印cycle信息
    if current_time != f64::INFINITY {
      println!("\n=== Cycle {} (时间 {:.1}) ===", current_time as i32, current_time);
    }

    if !messages.is_empty() {
      println!("{} 条消息:", messages.len());
      for msg in &messages {
        println!("  {} -> {} [{}]: {}",
          msg.source_id(),
          msg.target_id(),
          msg.target_port(),
          msg.content()
        );
      }
    } else if current_time != f64::INFINITY {
      println!("无消息");
    }

    self.step_count += 1;

    // 只有时间到达无穷大才认为模拟结束
    if current_time == f64::INFINITY {
      return Ok(false);
    }

    Ok(true)
  }

  fn print_summary(&self) {
    println!("\n=== 模拟总结 ===");
    println!("总步数: {}", self.step_count);
    println!("模拟时间: {:.2}", self.simulation.get_global_time());
  }

  /// 单步执行（公开接口）
  pub fn step(&mut self) -> io::Result<bool> {
    match self.execute_step() {
      Ok(result) => Ok(result),
      Err(e) => {
        eprintln!("模拟错误: {:?}", e);
        Ok(false)
      }
    }
  }

  /// 获取当前模拟时间
  pub fn get_time(&self) -> f64 {
    self.simulation.get_global_time()
  }
}
