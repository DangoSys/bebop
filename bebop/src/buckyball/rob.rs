use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use std::sync::atomic::{AtomicBool, Ordering};

pub static ROB_READY_TO_RECEIVE: AtomicBool = AtomicBool::new(true);
use crate::buckyball::decoder::send_cmd_response;
use crate::buckyball::decoder::FENCE_CSR;
use crate::buckyball::tdma_loader::MVIN_INST_CAN_ISSUE;
use crate::buckyball::tdma_storer::MVOUT_INST_CAN_ISSUE;
use crate::buckyball::vecball::VECBALL_INST_CAN_ISSUE;

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
enum EntryStatus {
  Allocated,
  Inflight,
  Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobEntry {
  funct: u64,
  xs1: u64,
  xs2: u64,
  domain_id: u64,
  status: EntryStatus,
  rob_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rob {
  capacity: u64,
  receive_inst_from_decoder_port: String,
  dispatch_to_rs_port: String,
  commit_port: String,
  rob_buffer: Vec<RobEntry>,
  until_next_event: f64,
  records: Vec<ModelRecord>,
}

impl Rob {
  pub fn new(
    capacity: u64,
    receive_inst_from_decoder_port: String,
    dispatch_to_rs_port: String,
    commit_port: String,
  ) -> Self {
    ROB_READY_TO_RECEIVE.store(true, Ordering::Relaxed);
    Self {
      capacity,
      receive_inst_from_decoder_port,
      dispatch_to_rs_port,
      commit_port,
      rob_buffer: init_rob(capacity),
      until_next_event: INFINITY,
      records: Vec::new(),
    }
  }
}

impl DevsModel for Rob {
  fn events_ext(&mut self, incoming_message: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    if incoming_message.port_name == self.receive_inst_from_decoder_port {
      let inst_values: Vec<u64> = serde_json::from_str(&incoming_message.content).unwrap();
      let funct = inst_values[0];
      let xs1 = inst_values[1];
      let xs2 = inst_values[2];
      let domain_id = inst_values[3];
      allocate_entry(&mut self.rob_buffer, funct, xs1, xs2, domain_id);
      self.until_next_event = 1.0;
    }

    if incoming_message.port_name == self.commit_port {
      let rob_id: u64 = serde_json::from_str(&incoming_message.content).unwrap();
      commit_entry(&mut self.rob_buffer, rob_id);
      self.until_next_event = 1.0;
    }

    ROB_READY_TO_RECEIVE.store(!is_full(&mut self.rob_buffer), Ordering::Relaxed);

    self.records.push(ModelRecord {
      time: services.global_time(),
      action: "receive".to_string(),
      subject: incoming_message.content.clone(),
    });

    Ok(())
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    if is_empty(&mut self.rob_buffer) {
      if FENCE_CSR.load(Ordering::Relaxed) {
        FENCE_CSR.store(false, Ordering::Relaxed);
        send_cmd_response(0u64);
        self.until_next_event = INFINITY;
      }
    } else {
      self.until_next_event = 1.0;
    }

    let (funct, xs1, xs2, domain_id, rob_id) = match dispatch_entry(&mut self.rob_buffer) {
      Some(entry) => entry,
      None => {
        self.until_next_event = INFINITY;
        return Ok(Vec::new());
      },
    };

    if !is_full(&mut self.rob_buffer) {
      ROB_READY_TO_RECEIVE.store(true, Ordering::Relaxed);
    }

    self.records.push(ModelRecord {
      time: services.global_time(),
      action: "dispatch".to_string(),
      subject: serde_json::to_string(&vec![funct as u64, xs1, xs2, domain_id as u64, rob_id as u64]).unwrap(),
    });

    Ok(vec![ModelMessage {
      content: serde_json::to_string(&vec![funct as u64, xs1, xs2, domain_id as u64, rob_id as u64]).unwrap(),
      port_name: self.dispatch_to_rs_port.clone(),
    }])
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    self.until_next_event
  }
}

impl Reportable for Rob {
  fn status(&self) -> String {
    let used = self.rob_buffer.iter().filter(|e| e.status != EntryStatus::Idle).count();
    format!("{}/{}", used, self.capacity)
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.records
  }
}

impl ReportableModel for Rob {}

impl SerializableModel for Rob {
  fn get_type(&self) -> &'static str {
    "Rob"
  }
}

/// ------------------------------------------------------------
/// --- Helper Functions ---
/// ------------------------------------------------------------
fn init_rob(capacity: u64) -> Vec<RobEntry> {
  let mut rob_buffer = Vec::new();
  for i in 0..capacity {
    rob_buffer.push(RobEntry {
      funct: 0,
      xs1: 0,
      xs2: 0,
      domain_id: 0,
      status: EntryStatus::Idle,
      rob_id: i,
    });
  }
  rob_buffer
}

/// allocate a new entry in the ROB, return the entry id
fn allocate_entry(rob_buffer: &mut Vec<RobEntry>, funct: u64, xs1: u64, xs2: u64, domain_id: u64) -> u64 {
  let rob_id = find_idle_entry(rob_buffer);
  let entry = &mut rob_buffer[rob_id as usize];
  entry.status = EntryStatus::Allocated;
  entry.rob_id = rob_id;
  entry.funct = funct;
  entry.xs1 = xs1;
  entry.xs2 = xs2;
  entry.domain_id = domain_id;
  rob_id
}

/// Finds the first entry from index 0 that is Allocated and marks it as Inflight
fn dispatch_entry(rob_buffer: &mut Vec<RobEntry>) -> Option<(u64, u64, u64, u64, u64)> {
  for entry in rob_buffer.iter_mut() {
    if entry.status == EntryStatus::Allocated {
      if check_can_issue(entry.funct) {
        entry.status = EntryStatus::Inflight;
        return Some((entry.funct, entry.xs1, entry.xs2, entry.domain_id, entry.rob_id));
      } else {
        continue;
      }
    }
  }
  None
}

/// commit an entry from the ROB (set it back to Idle)
fn commit_entry(rob_buffer: &mut Vec<RobEntry>, rob_id: u64) {
  for entry in rob_buffer.iter_mut() {
    if entry.rob_id == rob_id {
      entry.status = EntryStatus::Idle;
      break;
    }
  }
}

/// find the first Idle entry in the ROB
fn find_idle_entry(rob_buffer: &mut Vec<RobEntry>) -> u64 {
  for entry in rob_buffer.iter_mut() {
    if entry.status == EntryStatus::Idle {
      return entry.rob_id;
    }
  }
  0
}

/// check if ROB is empty (all entries are Idle)
fn is_empty(rob_buffer: &Vec<RobEntry>) -> bool {
  rob_buffer.iter().all(|entry| entry.status == EntryStatus::Idle)
}

/// check if ROB is full (all entries are Allocated)
fn is_full(rob_buffer: &Vec<RobEntry>) -> bool {
  rob_buffer.iter().all(|entry| entry.status != EntryStatus::Idle)
}

fn check_can_issue(funct: u64) -> bool {
  match funct {
    24 => MVIN_INST_CAN_ISSUE.load(Ordering::Relaxed),
    25 => MVOUT_INST_CAN_ISSUE.load(Ordering::Relaxed),
    30 => VECBALL_INST_CAN_ISSUE.load(Ordering::Relaxed),
    _ => false,
  }
}
