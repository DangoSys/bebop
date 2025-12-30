use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

use super::unit::decode::decode_instruction;
use crate::buckyball::frontend::bundles::rocc_frontend::RoccInstruction;
use crate::{log_backward, log_forward};
use bebop_lib::msg::{create_message, receive_message};
use bebop_lib::port_type::{InputPort, PortState};

/// Decoder模块 - 解码指令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decoder {
  // received messages
  decode_inst: InputPort,
  enter_rob: String,

  // State 字段
  events: Vec<DecoderEvent>,
  until_next_event: f64,
  records: Vec<ModelRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum DecoderEvent {
  Decode(RoccInstruction),
  CmdResponse(u64),
}

impl Decoder {
  pub fn new() -> Self {
    Self {
      decode_inst: InputPort::new("frontend_decoder".to_string()),
      enter_rob: "decoder_rob".to_string(),
      events: Vec::new(),
      until_next_event: INFINITY,
      records: Vec::new(),
    }
  }
}

impl DevsModel for Decoder {
  // events_ext: 被请求时当场调用，看是否塞入事件给这个模块
  // events_int: 随时间步抵达时调用的函数
  // 模拟器会为每个消息分别调用一次 events_ext, 根据 port_name 区分不同类型的消息
  fn events_ext(&mut self, received_msg: &ModelMessage, _services: &mut Services) -> Result<(), SimulationError> {
    match received_msg.port_name.as_str() {
      "frontend_decoder" => {
        if !self.decode_inst.is_idle() {
          log_backward!("Decoder: port {} is busy, rejecting message", self.decode_inst.name);
          return Ok(());
        }

        if let Ok(raw_inst) = receive_message::<RoccInstruction>(received_msg, &self.decode_inst.name) {
          log_forward!("Decoder: decode instruction funct={}, xs1={}, xs2={}", raw_inst.funct, raw_inst.xs1, raw_inst.xs2);
          self.events.push(DecoderEvent::Decode(raw_inst));
          self.until_next_event = 1.0;
        }
      },
      _ => {
        log_forward!("Decoder: received message from unknown port: {}", received_msg.port_name);
      },
    }
    Ok(())
  }

  fn events_int(&mut self, _services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut msg_output = Vec::new();
    for event in self.events.drain(..) {
      match event {
        DecoderEvent::Decode(raw_inst) => {
          let decoded_inst = decode_instruction(raw_inst);
          msg_output.push(create_message(&decoded_inst, &self.enter_rob)?);
          
          self.decode_inst.set_busy();
          log_backward!("Decoder: instruction decoded, port {} set to idle", self.decode_inst.name);
        },

        DecoderEvent::CmdResponse(response_data) => {
          log_backward!("Decoder: CmdResponse data={:#x}", response_data);
        },
      }
    }
    
    if self.events.is_empty() {
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

impl Reportable for Decoder {
  fn status(&self) -> String {
    String::new()
  }

  fn records(&self) -> &Vec<ModelRecord> {
    static EMPTY: Vec<ModelRecord> = Vec::new();
    &EMPTY
  }
}

impl ReportableModel for Decoder {}

impl SerializableModel for Decoder {
  fn get_type(&self) -> &'static str {
    "Decoder"
  }
}
