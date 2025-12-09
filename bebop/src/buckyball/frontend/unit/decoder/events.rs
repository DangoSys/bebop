use serde::{Deserialize, Serialize};
use sim::simulator::Services;
use sim::models::{ModelRecord, ModelMessage};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

use crate::{log_forward, log_backward};
use crate::buckyball::frontend::bundles::rocc_frontend::RoccInstruction;
use crate::buckyball::frontend::unit::rob::bundles::decoder_rob::DecodedInstruction;
use super::unit::decode::decode_funct;

/// Decoder模块 - 解码指令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decoder {
  ports_in: PortsIn,
  ports_out: PortsOut,
  state: State,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortsIn {
  decode_inst: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortsOut {
  enter_rob: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
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
      ports_in: PortsIn {
        decode_inst: "frontend_decoder".to_string(),},
      ports_out: PortsOut {
        enter_rob: "decoder_rob".to_string(),},
      state: State {
        events: Vec::new(),
        until_next_event: INFINITY,
        records: Vec::new(),
      },
    }
  }
}

impl DevsModel for Decoder {
  fn events_ext( &mut self, received_msg: &ModelMessage, _services: &mut Services,) -> Result<(), SimulationError> {
    // -----------------------------------------------------
    // receive instruction from frontend
    // -----------------------------------------------------
    if received_msg.port_name == self.ports_in.decode_inst {
      let raw_inst: RoccInstruction = serde_json::from_str(&received_msg.content)?;
      log_forward!("Decoder: funct={}, xs1={:#x}, xs2={:#x}", raw_inst.funct, raw_inst.xs1, raw_inst.xs2);
      self.state.events.push(DecoderEvent::Decode(raw_inst));
      self.state.until_next_event = 1.0;
    }
    Ok(())
  }
  fn events_int( &mut self, _services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut msg_output = Vec::new();
    for event in self.state.events.drain(..) {
      match event {
        // -----------------------------------------------------
        // process decoded instructions
        // -----------------------------------------------------
        DecoderEvent::Decode(raw_inst) => {
          let decoded_inst = DecodedInstruction::new(raw_inst.funct, raw_inst.xs1, raw_inst.xs2, decode_funct(raw_inst.funct));
          msg_output.push(ModelMessage {port_name: self.ports_out.enter_rob.clone(), content: serde_json::to_string(&decoded_inst)?,});
          log_backward!("Decoder: funct={} -> domain={}", raw_inst.funct, decoded_inst.domain_id);
        }
        // -----------------------------------------------------
        // Output command response to frontend
        // -----------------------------------------------------
        DecoderEvent::CmdResponse(response_data) => {
          log_backward!("Decoder: CmdResponse data={:#x}", response_data);
        }
      }
    }
    self.state.until_next_event = INFINITY;
    Ok(msg_output)
  }
  fn time_advance(&mut self, time_delta: f64) { self.state.until_next_event -= time_delta; }
  fn until_next_event(&self) -> f64 { self.state.until_next_event }
}

impl Reportable for Decoder {
  fn status(&self) -> String { format!("Decoder - Events: {}", self.state.events.len()) }
  fn records(&self) -> &Vec<ModelRecord> { &self.state.records }
}
impl ReportableModel for Decoder {}
impl SerializableModel for Decoder {
  fn get_type(&self) -> &'static str { "Decoder" }
}
