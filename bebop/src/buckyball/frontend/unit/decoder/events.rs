use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

use super::unit::decode::decode_funct;
use crate::buckyball::frontend::bundles::rocc_frontend::RoccInstruction;
use crate::buckyball::frontend::unit::rob::bundles::decoder_rob::DecodedInstruction;
use crate::log_backward;
use bebop_lib::msg::{create_message, receive_message};

/// Decoder模块 - 解码指令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decoder {
  // received messages
  decode_inst: String,
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
      decode_inst: "frontend_decoder".to_string(),
      enter_rob: "decoder_rob".to_string(),
      events: Vec::new(),
      until_next_event: INFINITY,
      records: Vec::new(),
    }
  }
}

impl DevsModel for Decoder {
  fn events_ext(&mut self, received_msg: &ModelMessage, _services: &mut Services) -> Result<(), SimulationError> {
    if let Ok(raw_inst) = receive_message::<RoccInstruction>(received_msg, &self.decode_inst) {
      self.events.push(DecoderEvent::Decode(raw_inst));
      self.until_next_event = 1.0;
    }
    Ok(())
  }

  fn events_int(&mut self, _services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut msg_output = Vec::new();
    for event in self.events.drain(..) {
      match event {
        DecoderEvent::Decode(raw_inst) => {
          // decode instruction
          let decoded_inst =
            DecodedInstruction::new(raw_inst.funct, raw_inst.xs1, raw_inst.xs2, decode_funct(raw_inst.funct));

          // send to ROB
          msg_output.push(create_message(&decoded_inst, &self.enter_rob)?);
        },

        DecoderEvent::CmdResponse(response_data) => {
          log_backward!("Decoder: CmdResponse data={:#x}", response_data);
        },
      }
    }
    self.until_next_event = INFINITY;
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
