use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Inst {
  funct: u64,
  xs1: u64,
  xs2: u64,
  domain_id: u64,
  rob_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rs {
  receive_inst_from_rob_port: String,
  issue_to_vecball_port: String,
  issue_to_tdma_mvin_port: String,
  issue_to_tdma_mvout_port: String,
  until_next_event: f64,
  records: Vec<ModelRecord>,
  inst_buffer: Vec<Inst>,
}

impl Rs {
  pub fn new(
    receive_inst_from_rob_port: String,
    issue_to_vecball_port: String,
    issue_to_tdma_mvin_port: String,
    issue_to_tdma_mvout_port: String,
  ) -> Self {
    Self {
      receive_inst_from_rob_port,
      issue_to_vecball_port,
      issue_to_tdma_mvin_port,
      issue_to_tdma_mvout_port,
      until_next_event: INFINITY,
      records: Vec::new(),
      inst_buffer: Vec::new(),
    }
  }
}

impl DevsModel for Rs {
  fn events_ext(&mut self, incoming_message: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    if incoming_message.port_name == self.receive_inst_from_rob_port {
      let inst_values: Vec<u64> = serde_json::from_str(&incoming_message.content).unwrap();
      let funct = inst_values[0];
      let xs1 = inst_values[1];
      let xs2 = inst_values[2];
      let domain_id = inst_values[3];
      let rob_id = inst_values[4];

      self.until_next_event = 1.0;

      push_to_buffer(&mut self.inst_buffer, funct, xs1, xs2, domain_id, rob_id);

      self.records.push(ModelRecord {
        time: services.global_time(),
        action: "receive".to_string(),
        subject: incoming_message.content.clone(),
      });
      Ok(())
    } else {
      Ok(())
    }
  }

  fn events_int(&mut self, _services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut messages = Vec::new();

    for inst in self.inst_buffer.drain(..) {
      let port_name = match inst.funct {
        24 => self.issue_to_tdma_mvin_port.clone(),
        25 => self.issue_to_tdma_mvout_port.clone(),
        30 => self.issue_to_vecball_port.clone(),
        _ => {
          return Err(SimulationError::InvalidModelState);
        },
      };
      let content = serde_json::to_string(&vec![inst.funct, inst.xs1, inst.xs2, inst.rob_id])
        .map_err(|_| SimulationError::InvalidModelState)?;
      messages.push(ModelMessage { content, port_name });
    }

    self.until_next_event = INFINITY;
    Ok(messages)
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    self.until_next_event
  }
}

impl Reportable for Rs {
  fn status(&self) -> String {
    "normal".to_string()
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.records
  }
}

impl ReportableModel for Rs {}

impl SerializableModel for Rs {
  fn get_type(&self) -> &'static str {
    "Rs"
  }
}

/// ------------------------------------------------------------
/// --- Helper Functions ---
/// ------------------------------------------------------------
fn push_to_buffer(inst_buffer: &mut Vec<Inst>, funct: u64, xs1: u64, xs2: u64, domain_id: u64, rob_id: u64) {
  inst_buffer.push(Inst {
    funct,
    xs1,
    xs2,
    domain_id,
    rob_id,
  });
}
