use serde::{Deserialize, Serialize};
use serde_json;
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::rob::ROB_READY_TO_RECEIVE;
use std::sync::mpsc::Sender;
static CMD_HANDLER: Mutex<Option<Arc<Mutex<crate::simulator::server::socket::CmdHandler>>>> = Mutex::new(None);
static RESP_TX: Mutex<Option<Sender<u64>>> = Mutex::new(None);
pub static FENCE_CSR: AtomicBool = AtomicBool::new(false);


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decoder {
  instruction_port: String,
  push_to_rob_port: String,
  until_next_event: f64,
  inst: Option<(u64, u64, u64)>,
  records: Vec<ModelRecord>,
}

impl Decoder {
  pub fn new(instruction_port: String, push_to_rob_port: String) -> Self {
    Self {
      instruction_port,
      push_to_rob_port,
      until_next_event: INFINITY,
      inst: None,
      records: Vec::new(),
    }
  }
}

impl DevsModel for Decoder {
  fn events_ext(&mut self, incoming_message: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    let inst_values: Vec<u64> = serde_json::from_str(&incoming_message.content).unwrap();
    let funct = inst_values[0];
    let xs1 = inst_values[1];
    let xs2 = inst_values[2];
    self.inst = Some((funct, xs1, xs2));

    // fence inst dont push to rob
    if funct == 31 {
      FENCE_CSR.store(true, Ordering::Relaxed);
      self.until_next_event = INFINITY;
    } else {
      self.until_next_event = 1.0;
    }
    Ok(())
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let (funct, xs1, xs2) = self.inst.unwrap();
      let rob_ready = ROB_READY_TO_RECEIVE.load(Ordering::Relaxed);

      if !rob_ready {
        self.inst = Some((funct, xs1, xs2));
        self.until_next_event = 1.0;
        return Ok(Vec::new());
      }

      if FENCE_CSR.load(Ordering::Relaxed) {
        self.until_next_event = 1.0;
        return Ok(Vec::new());
      }

      self.until_next_event = INFINITY;
      
      let domain_id = decode_funct(funct);

      let mut messages = Vec::new();
      let msg_rob = ModelMessage {
        content: serde_json::to_string(&vec![funct, xs1, xs2, domain_id]).unwrap(),
        port_name: self.push_to_rob_port.clone(),
      };
      messages.push(msg_rob);

      send_cmd_response(0u64);

      Ok(messages)
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
    if self.inst.is_some() {
      "busy".to_string()
    } else {
      "idle".to_string()
    }
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.records
  }
}

impl ReportableModel for Decoder {}

impl SerializableModel for Decoder {
  fn get_type(&self) -> &'static str {
    "Decoder"
  }
}

/// ------------------------------------------------------------
/// --- Helper Functions ---
/// ------------------------------------------------------------
fn decode_funct(funct: u64) -> u64 {
  let domain_id = match funct {
    31 => 0,      // Fence -> domain 0
    24 | 25 => 1, // Load -> domain 1 (memdomain)
    _ => 2,       // Compute -> domain 2 (balldomain),
  };
  domain_id
}

pub fn set_cmd_handler(handler: Arc<Mutex<crate::simulator::server::socket::CmdHandler>>) {
  *CMD_HANDLER.lock().unwrap() = Some(handler);
}

pub fn set_resp_tx(resp_tx: Sender<u64>) {
  *RESP_TX.lock().unwrap() = Some(resp_tx);
}

pub fn send_cmd_response(result: u64) {
  let resp_tx_opt = RESP_TX.lock().unwrap();
  if let Some(resp_tx) = resp_tx_opt.as_ref() {
    if resp_tx.send(result).is_err() {
      eprintln!("[Decoder] Failed to send response through channel");
    }
  }
}


/// ------------------------------------------------------------
/// --- Test Functions ---
/// ------------------------------------------------------------
#[test]
fn test_decode_funct() {
  assert_eq!(decode_funct(31), 0);
  assert_eq!(decode_funct(24), 1);
  assert_eq!(decode_funct(25), 1);
  assert_eq!(decode_funct(26), 2);
}
