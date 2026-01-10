use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::simulator::server::socket::{DmaReadHandler, DmaWriteHandler};

// Global DMA handlers, set during initialization
static DMA_READ_HANDLER: Mutex<Option<Arc<Mutex<DmaReadHandler>>>> = Mutex::new(None);
static DMA_WRITE_HANDLER: Mutex<Option<Arc<Mutex<DmaWriteHandler>>>> = Mutex::new(None);

pub static MVIN_INST_CAN_ISSUE: AtomicBool = AtomicBool::new(true);
pub static MVOUT_INST_CAN_ISSUE: AtomicBool = AtomicBool::new(true);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tdma {
  mvin_req_port: String,
  mvout_req_port: String,
  read_bank_req_port: String,
  write_bank_req_port: String,
  read_bank_resp_port: String,
  write_bank_resp_port: String,
  commit_to_rob_port: String,
  until_next_event: f64,
  records: Vec<ModelRecord>,

  // mvout
  current_bank_read_iter: u64,
  all_bank_read_iter: u64,
  mvout_base_dram_addr: u64,
  mvout_stride: u64,
  mvout_vbank_id: u64,
  current_mvout_bank_addr: u64,
  current_mvout_dram_addr: u64,
  mvout_rob_id: u64,
  mvout_read_pending: bool, // Track if we're waiting for a read response

  // mvin
  current_bank_write_iter: u64,
  all_bank_write_iter: u64,
  mvin_base_dram_addr: u64,
  mvin_stride: u64,
  mvin_vbank_id: u64,
  current_mvin_bank_addr: u64,
  current_mvin_dram_addr: u64,
  mvin_rob_id: u64,
}

impl Tdma {
  pub fn new(
    mvin_req_port: String,
    mvout_req_port: String,
    read_bank_req_port: String,
    write_bank_req_port: String,
    read_bank_resp_port: String,
    write_bank_resp_port: String,
    commit_to_rob_port: String,
  ) -> Self {
    MVIN_INST_CAN_ISSUE.store(true, Ordering::Relaxed);
    MVOUT_INST_CAN_ISSUE.store(true, Ordering::Relaxed);
    Self {
      mvin_req_port,
      mvout_req_port,
      read_bank_req_port,
      write_bank_req_port,
      read_bank_resp_port,
      write_bank_resp_port,
      commit_to_rob_port,
      until_next_event: INFINITY,
      records: Vec::new(),
      current_bank_read_iter: 0,
      all_bank_read_iter: 0,
      mvout_base_dram_addr: 0,
      mvout_stride: 0,
      mvout_vbank_id: 0,
      mvout_rob_id: 0,
      mvout_read_pending: false, // Initialize to false
      current_bank_write_iter: 0,
      all_bank_write_iter: 0,
      mvin_base_dram_addr: 0,
      mvin_stride: 0,
      mvin_vbank_id: 0,
      mvin_rob_id: 0,
      current_mvout_bank_addr: 0,
      current_mvout_dram_addr: 0,
      current_mvin_bank_addr: 0,
      current_mvin_dram_addr: 0,
    }
  }
}

impl DevsModel for Tdma {
  fn events_ext(&mut self, _incoming_message: &ModelMessage, _services: &mut Services) -> Result<(), SimulationError> {
    if _incoming_message.port_name == self.mvin_req_port {
      let (base_dram_addr, stride, depth, vbank_id, rob_id) = decode_inst(&_incoming_message.content);
      self.current_bank_write_iter = 0;
      self.all_bank_write_iter = depth;
      self.current_mvin_bank_addr = 0;
      self.mvin_base_dram_addr = base_dram_addr;
      self.current_mvin_dram_addr = base_dram_addr;
      self.mvin_stride = stride;
      self.mvin_vbank_id = vbank_id;
      self.mvin_rob_id = rob_id;
      MVIN_INST_CAN_ISSUE.store(false, Ordering::Relaxed);
    }

    if _incoming_message.port_name == self.mvout_req_port {
      let (base_dram_addr, stride, depth, vbank_id, rob_id) = decode_inst(&_incoming_message.content);
      self.current_bank_read_iter = 0;
      self.all_bank_read_iter = depth;
      self.current_mvout_bank_addr = 0;
      self.mvout_base_dram_addr = base_dram_addr;
      self.current_mvout_dram_addr = base_dram_addr;
      self.mvout_stride = stride;
      self.mvout_vbank_id = vbank_id;
      self.mvout_rob_id = rob_id;
      self.mvout_read_pending = false; // Reset pending flag for new MVOUT instruction
      MVOUT_INST_CAN_ISSUE.store(false, Ordering::Relaxed);
    }

    if _incoming_message.port_name == self.read_bank_resp_port {
      let data_values: Vec<u128> = serde_json::from_str(&_incoming_message.content).unwrap();
      let data = data_values[0];
      // Calculate address for the iteration that just completed
      // The response corresponds to the request sent when current_bank_read_iter was (current - 1)
      let completed_iter = self.current_bank_read_iter;
      let write_addr = self.mvout_base_dram_addr + completed_iter * 16 * self.mvout_stride;
      dma_write_dram(write_addr, data);

      self.current_bank_read_iter += 1;
      self.mvout_read_pending = false; // Clear pending flag when response arrives

      if self.current_bank_read_iter == self.all_bank_read_iter {
        MVOUT_INST_CAN_ISSUE.store(true, Ordering::Relaxed);
      }
    }

    if _incoming_message.port_name == self.write_bank_resp_port {
      self.current_bank_write_iter += 1;

      if self.current_bank_write_iter == self.all_bank_write_iter {
        MVIN_INST_CAN_ISSUE.store(true, Ordering::Relaxed);
      }
    }

    self.until_next_event = 1.0;
    Ok(())
  }

  fn events_int(&mut self, _services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut messages = Vec::new();
    let mut has_work = false;

    // MVOUT: read from bank (bank -> DRAM)
    if !MVOUT_INST_CAN_ISSUE.load(Ordering::Relaxed)
      && self.current_bank_read_iter < self.all_bank_read_iter
      && !self.mvout_read_pending
    {
      messages.push(ModelMessage {
        content: serde_json::to_string(&vec![self.mvout_vbank_id, self.current_bank_read_iter]).unwrap(),
        port_name: self.read_bank_req_port.clone(),
      });
      self.mvout_read_pending = true; // Mark that we're waiting for a response
      self.until_next_event = 1.0;
      has_work = true;
    }

    // MVIN: write to bank (DRAM -> bank)
    if !MVIN_INST_CAN_ISSUE.load(Ordering::Relaxed) && self.current_bank_write_iter < self.all_bank_write_iter {
      // Calculate address for current iteration before reading
      let current_addr = self.mvin_base_dram_addr + self.current_bank_write_iter * 16 * self.mvin_stride;
      let (data_lo, data_hi) = dma_read_dram(current_addr);
      messages.push(ModelMessage {
        content: serde_json::to_string(&vec![
          self.mvin_vbank_id,
          self.current_bank_write_iter,
          data_lo,
          data_hi,
        ])
        .unwrap(),
        port_name: self.write_bank_req_port.clone(),
      });
      self.until_next_event = 1.0;
      has_work = true;
    }

    // MVOUT commit: send rob_id to ROB when mvout completes
    if MVOUT_INST_CAN_ISSUE.load(Ordering::Relaxed)
      && self.current_bank_read_iter == self.all_bank_read_iter
      && self.all_bank_read_iter > 0
    {
      messages.push(ModelMessage {
        content: serde_json::to_string(&self.mvout_rob_id).unwrap(),
        port_name: self.commit_to_rob_port.clone(),
      });
      self.all_bank_read_iter = 0; // Reset to avoid re-sending
      has_work = true;
    }

    // MVIN commit: send rob_id to ROB when mvin completes
    if MVIN_INST_CAN_ISSUE.load(Ordering::Relaxed)
      && self.current_bank_write_iter == self.all_bank_write_iter
      && self.all_bank_write_iter > 0
    {
      messages.push(ModelMessage {
        content: serde_json::to_string(&self.mvin_rob_id).unwrap(),
        port_name: self.commit_to_rob_port.clone(),
      });
      self.all_bank_write_iter = 0; // Reset to avoid re-sending
      has_work = true;
    }

    if !has_work {
      self.until_next_event = INFINITY;
    }

    Ok(messages)
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    self.until_next_event
  }
}

impl Reportable for Tdma {
  fn status(&self) -> String {
    "idle".to_string()
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.records
  }
}

impl ReportableModel for Tdma {}

impl SerializableModel for Tdma {
  fn get_type(&self) -> &'static str {
    "Tdma"
  }
}

/// ------------------------------------------------------------
/// --- Helper Functions ---
/// ------------------------------------------------------------
fn decode_inst(inst: &str) -> (u64, u64, u64, u64, u64) {
  let inst_values: Vec<u64> = serde_json::from_str(inst).unwrap();
  let xs1 = inst_values[1];
  let xs2 = inst_values[2];
  let rob_id = inst_values[3];

  let base_dram_addr = (xs1 & 0xffffffff) as u64;
  let stride = ((xs2 >> 24) & 0x3ff) as u64;
  let depth = ((xs2 >> 8) & 0xffff) as u64;
  let vbank_id = (xs2 & 0xff) as u64;
  (base_dram_addr, stride, depth, vbank_id, rob_id)
}

pub fn set_dma_read_handler(handler: Arc<Mutex<crate::simulator::server::socket::DmaReadHandler>>) {
  *DMA_READ_HANDLER.lock().unwrap() = Some(handler);
}

pub fn set_dma_write_handler(handler: Arc<Mutex<crate::simulator::server::socket::DmaWriteHandler>>) {
  *DMA_WRITE_HANDLER.lock().unwrap() = Some(handler);
}

fn dma_read_dram(dram_addr: u64) -> (u64, u64) {
  let handler_opt = DMA_READ_HANDLER.lock().unwrap();
  if let Some(handler) = handler_opt.as_ref() {
    let mut h = handler.lock().unwrap();
    let data = h.read(dram_addr, 16).unwrap_or(0);
    let data_lo = data as u64;
    let data_hi = (data >> 64) as u64;
    (data_lo, data_hi)
  } else {
    (0, 0)
  }
}

fn dma_write_dram(dram_addr: u64, data: u128) {
  let handler_opt = DMA_WRITE_HANDLER.lock().unwrap();
  if let Some(handler) = handler_opt.as_ref() {
    let mut h = handler.lock().unwrap();
    let _ = h.write(dram_addr, data, 16);
  }
}
