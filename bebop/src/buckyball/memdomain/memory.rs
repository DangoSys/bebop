use serde::{Deserialize, Serialize};
use sim::simulator::Services;
use sim::models::{ModelRecord, DevsModel, Reportable, ReportableModel, ModelMessage};
use sim::models::model_trait::SerializableModel;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

/// Memory模块 - 处理读写请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
  ports_in: PortsIn,
  ports_out: PortsOut,
  state: State,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortsIn {
  request: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortsOut {
  response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
  phase: Phase,
  until_next_event: f64,
  records: Vec<ModelRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum Phase {
  Idle,
  Processing,
}

impl Memory {
  pub fn new() -> Self {
    Self {
      ports_in: PortsIn {
        request: "request".to_string(),
      },
      ports_out: PortsOut {
        response: "response".to_string(),
      },
      state: State {
        phase: Phase::Idle,
        until_next_event: INFINITY,
        records: Vec::new(),
      },
    }
  }
}

impl DevsModel for Memory {
  fn events_ext(
    &mut self,
    incoming_message: &ModelMessage,
    _services: &mut Services,
  ) -> Result<(), SimulationError> {
    if incoming_message.port_name == self.ports_in.request {
      // 收到内存请求
      self.state.phase = Phase::Processing;
      self.state.until_next_event = 1.0; // 模拟内存访问延迟1个cycle
    }
    Ok(())
  }

  fn events_int(
    &mut self,
    _services: &mut Services,
  ) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut messages = Vec::new();
    
    if self.state.phase == Phase::Processing {
      // 发送内存响应
      messages.push(ModelMessage {
        port_name: self.ports_out.response.clone(),
        content: "DATA_READY".to_string(),
      });
      
      self.state.phase = Phase::Idle;
      self.state.until_next_event = INFINITY;
    }
    
    Ok(messages)
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.state.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    self.state.until_next_event
  }
}

impl Reportable for Memory {
  fn status(&self) -> String {
    format!("Memory - Phase: {:?}", self.state.phase)
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.state.records
  }
}

impl ReportableModel for Memory {}

impl SerializableModel for Memory {
  fn get_type(&self) -> &'static str {
    "Memory"
  }
}
