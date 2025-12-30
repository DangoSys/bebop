use serde::{Deserialize, Serialize};
use sim::models::model_trait::SerializableModel;
use sim::models::{DevsModel, ModelMessage, ModelRecord, Reportable, ReportableModel};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use bebop_lib::msg::create_message;

/// Memdomain模块 - 处理读写请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memdomain {
  // PortsIn 字段
  request: String,
  // PortsOut 字段
  response: String,
  // State 字段
  phase: Phase,
  until_next_event: f64,
  records: Vec<ModelRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum Phase {
  Idle,
  Processing,
}

impl Memdomain {
  pub fn new() -> Self {
    Self {
      request: "balldomain_memdomain".to_string(),
      response: "memdomain_balldomain".to_string(),
      phase: Phase::Idle,
      until_next_event: INFINITY,
      records: Vec::new(),
    }
  }
}

impl DevsModel for Memdomain {
  fn events_ext(&mut self, msg_input: &ModelMessage, _services: &mut Services) -> Result<(), SimulationError> {
    if msg_input.port_name == self.request {
      // 收到内存请求
      self.phase = Phase::Processing;
      self.until_next_event = 1.0; // 模拟内存访问延迟1个cycle
    }
    Ok(())
  }

  fn events_int(&mut self, _services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut msg_output = Vec::new();

    if self.phase == Phase::Processing {
      // 发送内存响应
      msg_output.push(create_message(&"DATA_READY".to_string(), &self.response)?);

      self.phase = Phase::Idle;
      self.until_next_event = INFINITY;
    }

    Ok(msg_output)
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    self.until_next_event
  }
}

impl Reportable for Memdomain {
  fn status(&self) -> String {
    String::new()
  }

  fn records(&self) -> &Vec<ModelRecord> {
    static EMPTY: Vec<ModelRecord> = Vec::new();
    &EMPTY
  }
}

impl ReportableModel for Memdomain {}

impl SerializableModel for Memdomain {
  fn get_type(&self) -> &'static str {
    "Memdomain"
  }
}
