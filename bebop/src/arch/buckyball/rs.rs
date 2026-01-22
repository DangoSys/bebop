use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

use super::mset::{receive_mset_inst, MSET_INST_CAN_ISSUE};
use super::systolic_array::{receive_systolic_array_inst, SYSTOLIC_ARRAY_INST_CAN_ISSUE};
use super::tdma_loader::{receive_mvin_inst, MVIN_INST_CAN_ISSUE};
use super::tdma_storer::{receive_mvout_inst, MVOUT_INST_CAN_ISSUE};
use super::vecball::{receive_vecball_inst, VECBALL_INST_CAN_ISSUE};
use std::sync::atomic::Ordering;

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
  until_next_event: f64,
  records: Vec<ModelRecord>,
  inst_buffer: Vec<Inst>,
}

impl Rs {
  pub fn new(receive_inst_from_rob_port: String) -> Self {
    Self {
      receive_inst_from_rob_port,
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
    for inst in self.inst_buffer.drain(..) {
      match inst.funct {
        23 => {
          if MSET_INST_CAN_ISSUE.load(Ordering::Relaxed) {
            receive_mset_inst(inst.xs1, inst.xs2, inst.rob_id);
          }
        },
        24 => {
          if MVIN_INST_CAN_ISSUE.load(Ordering::Relaxed) {
            receive_mvin_inst(inst.xs1, inst.xs2, inst.rob_id);
          }
        },
        25 => {
          if MVOUT_INST_CAN_ISSUE.load(Ordering::Relaxed) {
            receive_mvout_inst(inst.xs1, inst.xs2, inst.rob_id);
          }
        },
        30 => {
          if VECBALL_INST_CAN_ISSUE.load(Ordering::Relaxed) {
            receive_vecball_inst(inst.xs1, inst.xs2, inst.rob_id);
          }
        },
        32 => {
          // Systolic array matrix multiplication instruction
          // For now, we'll use xs1, xs2, and domain_id to encode the bank IDs and dimensions
          // In a real system, these would be extracted from register values or immediate fields
          if SYSTOLIC_ARRAY_INST_CAN_ISSUE.load(Ordering::Relaxed) {
            let op1_bank_id = inst.xs1;
            let op2_bank_id = inst.xs2;
            let wr_bank_id = (inst.domain_id >> 24) & 0xFF;
            let m_dim = (inst.domain_id >> 16) & 0xFF;
            let n_dim = (inst.domain_id >> 8) & 0xFF;
            let k_dim = inst.domain_id & 0xFF;
            
            receive_systolic_array_inst(
              op1_bank_id, 
              op2_bank_id, 
              wr_bank_id, 
              m_dim, 
              n_dim, 
              k_dim, 
              inst.rob_id
            );
          }
        },
        _ => {
          return Err(SimulationError::InvalidModelState);
        },
      }
    }

    self.until_next_event = INFINITY;
    Ok(Vec::new())
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
