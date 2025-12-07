use serde::{Deserialize, Serialize};
use sim::simulator::Services;
use sim::models::{ModelRecord, ModelMessage};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

/// Decoder模块 - 解码指令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decoder {
  ports_in: PortsIn,
  ports_out: PortsOut,
  state: State,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortsIn {
  instruction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortsOut {
  decoded: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
  phase: Phase,
  until_next_event: f64,
  current_instruction: Option<String>,
  records: Vec<ModelRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum Phase {
  Idle,
  Decoding,
}

impl Decoder {
  pub fn new() -> Self {
    Self {
      ports_in: PortsIn {
        instruction: "instruction".to_string(),
      },
      ports_out: PortsOut {
        decoded: "decoded".to_string(),
      },
      state: State {
        phase: Phase::Idle,
        until_next_event: INFINITY,
        current_instruction: None,
        records: Vec::new(),
      },
    }
  }
}

impl DevsModel for Decoder {
  fn events_ext(
    &mut self,
    incoming_message: &ModelMessage,
    services: &mut Services,
  ) -> Result<(), SimulationError> {
    if incoming_message.port_name == self.ports_in.instruction {
      // 收到指令，开始解码
      self.state.phase = Phase::Decoding;
      self.state.current_instruction = Some(incoming_message.content.clone());
      self.state.until_next_event = 1.0; // 解码延迟1个cycle
      
      self.state.records.push(ModelRecord {
        subject: "Decoder".to_string(),
        time: services.global_time(),
        action: format!("开始解码: {}", incoming_message.content),
      });
    }
    Ok(())
  }

  fn events_int(
    &mut self,
    services: &mut Services,
  ) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut messages = Vec::new();
    
    if self.state.phase == Phase::Decoding {
      if let Some(inst) = &self.state.current_instruction {
        // 解码完成，发送到ROB
        let decoded = format!("DECODED_{}", inst);
        messages.push(ModelMessage {
          port_name: self.ports_out.decoded.clone(),
          content: decoded.clone(),
        });
        
        self.state.records.push(ModelRecord {
          subject: "Decoder".to_string(),
          time: services.global_time(),
          action: format!("解码完成: {}", decoded),
        });
      }
      
      self.state.phase = Phase::Idle;
      self.state.current_instruction = None;
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

impl Reportable for Decoder {
  fn status(&self) -> String {
    format!("Decoder - Phase: {:?}", self.state.phase)
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.state.records
  }
}

impl ReportableModel for Decoder {}

impl SerializableModel for Decoder {
  fn get_type(&self) -> &'static str {
    "Decoder"
  }
}
