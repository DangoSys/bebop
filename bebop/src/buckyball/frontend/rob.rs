use serde::{Deserialize, Serialize};
use sim::simulator::Services;
use sim::models::{ModelRecord, ModelMessage};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

/// ROB (Reorder Buffer) 模块 - 重排序缓冲区
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rob {
  ports_in: PortsIn,
  ports_out: PortsOut,
  state: State,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortsIn {
  decoded: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortsOut {
  to_compute: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
  phase: Phase,
  until_next_event: f64,
  buffer: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum Phase {
  Idle,
  Dispatching,
}

impl Rob {
  pub fn new() -> Self {
    Self {
      ports_in: PortsIn {
        decoded: "decoded".to_string(),
      },
      ports_out: PortsOut {
        to_compute: "to_compute".to_string(),
      },
      state: State {
        phase: Phase::Idle,
        until_next_event: INFINITY,
        buffer: Vec::new(),
      },
    }
  }
}

impl DevsModel for Rob {
  fn events_ext(
    &mut self,
    incoming_message: &ModelMessage,
    _services: &mut Services,
  ) -> Result<(), SimulationError> {
    if incoming_message.port_name == self.ports_in.decoded {
      // 收到解码后的指令，直接放入缓冲区
      self.state.buffer.push(incoming_message.content.clone());
      
      // 如果之前是Idle，现在有任务了，切换到Dispatching并调度
      if self.state.phase == Phase::Idle {
        self.state.phase = Phase::Dispatching;
        self.state.until_next_event = 1.0; // 调度延迟1个cycle
      }
    }
    Ok(())
  }

  fn events_int(
    &mut self,
    _services: &mut Services,
  ) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut messages = Vec::new();
    
    if self.state.phase == Phase::Dispatching && !self.state.buffer.is_empty() {
      // 从缓冲区取出指令发送到Compute
      let instruction = self.state.buffer.remove(0);
      messages.push(ModelMessage {
        port_name: self.ports_out.to_compute.clone(),
        content: format!("TASK_{}", instruction),
      });
      
      // 如果缓冲区还有指令，继续派发
      if !self.state.buffer.is_empty() {
        self.state.until_next_event = 1.0; // 1个cycle后继续派发
      } else {
        self.state.phase = Phase::Idle;
        self.state.until_next_event = INFINITY;
      }
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

impl Reportable for Rob {
  fn status(&self) -> String {
    format!("ROB - Phase: {:?}, Buffer: {}", 
      self.state.phase, self.state.buffer.len())
  }

  fn records(&self) -> &Vec<ModelRecord> {
    // ROB不记录详细日志
    static EMPTY: Vec<ModelRecord> = Vec::new();
    &EMPTY
  }
}

impl ReportableModel for Rob {}

impl SerializableModel for Rob {
  fn get_type(&self) -> &'static str {
    "ROB"
  }
}
