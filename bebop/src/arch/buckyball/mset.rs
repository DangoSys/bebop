use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use super::bmt;
use crate::model_record;

pub static MSET_INST_CAN_ISSUE: AtomicBool = AtomicBool::new(true);

struct MsetInstData {
  xs1: u64,
  xs2: u64,
  rob_id: u64,
}

static MSET_INST_DATA: Mutex<Option<MsetInstData>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mset {
  commit_to_rob_port: String,
  until_next_event: f64,
  records: Vec<ModelRecord>,
}

impl Mset {
  pub fn new(commit_to_rob_port: String) -> Self {
    MSET_INST_CAN_ISSUE.store(true, Ordering::Relaxed);
    *MSET_INST_DATA.lock().unwrap() = None;
    Self {
      commit_to_rob_port,
      until_next_event: INFINITY,
      records: Vec::new(),
    }
  }
}

impl DevsModel for Mset {
  fn events_ext(&mut self, _incoming_message: &ModelMessage, _services: &mut Services) -> Result<(), SimulationError> {
    Ok(())
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut messages = Vec::new();

    if let Some(inst) = MSET_INST_DATA.lock().unwrap().take() {
      // Decode and process MSET instruction
      let (release_en, vbank_id, alloc_en, row, col) = decode_mset(inst.xs1, inst.xs2);

      let success = if release_en {
        bmt::free_bank(vbank_id)
      } else if alloc_en {
        let num_pbanks = row * col;
        if num_pbanks == 0 {
          false
        } else {
          bmt::allocate_bank(vbank_id, num_pbanks).is_some()
        }
      } else {
        false
      };

      model_record!(
        self,
        services,
        if release_en { "release_bank" } else { "alloc_bank" },
        format!("vbank_id={}, bank_num={}, success={}", vbank_id, row * col, success)
      );

      messages.push(ModelMessage {
        content: serde_json::to_string(&inst.rob_id).map_err(|_| SimulationError::InvalidModelState)?,
        port_name: self.commit_to_rob_port.clone(),
      });

      model_record!(self, services, "commit_mset", format!("rob_id={}", inst.rob_id));
      MSET_INST_CAN_ISSUE.store(true, Ordering::Relaxed);
      self.until_next_event = INFINITY;
    } else {
      self.until_next_event = INFINITY;
    }

    Ok(messages)
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    if MSET_INST_DATA.lock().unwrap().is_some() {
      return 0.0;
    }
    self.until_next_event
  }
}

impl Reportable for Mset {
  fn status(&self) -> String {
    "normal".to_string()
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.records
  }
}

impl ReportableModel for Mset {}

impl SerializableModel for Mset {
  fn get_type(&self) -> &'static str {
    "Mset"
  }
}
/// ------------------------------------------------------------
/// --- Helper Functions ---
/// ------------------------------------------------------------
/// Decode MSET instruction fields
/// xs1: bit 0 = release_en, bit 1-13 = bank_id (vbank_id)
/// xs2: bit 0 = alloc_en, bit 1-5 = row, bit 6-13 = col
fn decode_mset(xs1: u64, xs2: u64) -> (bool, u64, bool, u64, u64) {
  let release_en = (xs1 & 0x1) != 0;
  let bank_id = (xs1 >> 1) & 0x1FFF; // bits 1-13
  let alloc_en = (xs2 & 0x1) != 0;
  let row = (xs2 >> 1) & 0x1F; // bits 1-5
  let col = (xs2 >> 6) & 0xFF; // bits 6-13
  (release_en, bank_id, alloc_en, row, col)
}

/// Receive MSET instruction (called by RS or other modules)
/// Caller should check MSET_INST_CAN_ISSUE before calling this function
pub fn receive_mset_inst(xs1: u64, xs2: u64, rob_id: u64) {
  *MSET_INST_DATA.lock().unwrap() = Some(MsetInstData { xs1, xs2, rob_id });
  MSET_INST_CAN_ISSUE.store(false, Ordering::Relaxed);
}
