use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use serde_json;
use super::rob::ROB_READY_TO_RECEIVE;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decoder {
  port_in: String,
  push_to_rob: String,  // port_out
  until_next_event: f64,
  inst: Option<(u64, u64, u64)>,
  records: Vec<ModelRecord>,
}

impl Decoder {
  pub fn new(port_in: String, port_out: String) -> Self {
    Self {
      port_in,
      push_to_rob: port_out.clone(),
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

    println!("[Decoder] events_ext: received instruction at t={:.1}: funct={}, xs1=0x{:x}, xs2=0x{:x}",
             services.global_time(), funct, xs1, xs2);
    
    self.until_next_event = 1.0;


    Ok(())
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    if let Some((funct, xs1, xs2)) = self.inst.take() {
      let rob_ready = *ROB_READY_TO_RECEIVE.lock().unwrap();
      
      if !rob_ready {
        println!("[Decoder] events_int: rob not ready, holding instruction at t={:.1}", services.global_time());
        self.inst = Some((funct, xs1, xs2));
        self.until_next_event = 1.0;
        return Ok(Vec::new());
      }

      self.until_next_event = INFINITY;
      let domain_id = decode_funct(funct);

      println!("[Decoder] events_int: rob ready, sending instruction at t={:.1}", services.global_time());
      let mut messages = Vec::new();
      let msg_rob = ModelMessage {
        content: serde_json::to_string(&vec![funct, xs1, xs2, domain_id]).unwrap(),
        port_name: self.push_to_rob.clone(),
      };
      messages.push(msg_rob);

      Ok(messages)
    } else {
      Ok(Vec::new())
    }
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
