use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::mem_ctrl::request_read_bank_for_tdma;
use crate::model_record;
use crate::simulator::server::socket::DmaWriteHandler;

// Global DMA handler, set during initialization
static DMA_WRITE_HANDLER: Mutex<Option<Arc<Mutex<DmaWriteHandler>>>> = Mutex::new(None);

pub static MVOUT_INST_CAN_ISSUE: AtomicBool = AtomicBool::new(true);

// Instruction data (set by receive_mvout_inst, cleared when processed)
struct MvoutInstData {
  base_dram_addr: u64,
  stride: u64,
  depth: u64,
  vbank_id: u64,
  rob_id: u64,
}

static MVOUT_INST_DATA: Mutex<Option<MvoutInstData>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum TdmaStorerState {
  Idle,
  Wait,     // Waiting for read bank response
  Active,   // Bank -> DRAM batch transfer in progress
  Complete, // Batch transfer complete
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TdmaStorer {
  read_bank_resp_port: String,
  commit_to_rob_port: String,

  state: TdmaStorerState,

  // MVOUT state (Bank -> DRAM)
  base_dram_addr: u64,
  stride: u64,
  depth: u64,
  vbank_id: u64,
  rob_id: u64,

  // Latency parameters
  transfer_latency: f64,
  until_next_event: f64,
  records: Vec<ModelRecord>,
}

impl TdmaStorer {
  pub fn new(read_bank_resp_port: String, commit_to_rob_port: String) -> Self {
    MVOUT_INST_CAN_ISSUE.store(true, Ordering::Relaxed);
    *MVOUT_INST_DATA.lock().unwrap() = None;
    Self {
      read_bank_resp_port,
      commit_to_rob_port,
      until_next_event: INFINITY,
      records: Vec::new(),
      state: TdmaStorerState::Idle,
      base_dram_addr: 0,
      stride: 0,
      depth: 0,
      vbank_id: 0,
      rob_id: 0,
      transfer_latency: 1.0,
    }
  }
}

impl DevsModel for TdmaStorer {
  fn events_ext(&mut self, incoming_message: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    if incoming_message.port_name == self.read_bank_resp_port {
      if self.state != TdmaStorerState::Wait {
        return Err(SimulationError::InvalidModelState);
      }

      let data_vec: Vec<u128> =
        serde_json::from_str(&incoming_message.content).map_err(|_| SimulationError::InvalidModelState)?;

      for (i, &data) in data_vec.iter().enumerate() {
        let dram_addr = self.base_dram_addr + (i as u64) * 16 * self.stride;
        dma_write_dram(dram_addr, data);
      }

      model_record!(self, services, "write_dram", format!("count={}", data_vec.len()));

      self.state = TdmaStorerState::Active;
      self.until_next_event = self.transfer_latency * self.depth as f64;

      return Ok(());
    }

    Ok(())
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut messages = Vec::new();

    match self.state {
      TdmaStorerState::Idle => {
        if let Some(inst) = MVOUT_INST_DATA.lock().unwrap().take() {
          self.base_dram_addr = inst.base_dram_addr;
          self.stride = inst.stride;
          self.depth = inst.depth;
          self.vbank_id = inst.vbank_id;
          self.rob_id = inst.rob_id;

          model_record!(
            self,
            services,
            "receive_inst",
            format!("dram_addr={:#x}, depth={}", inst.base_dram_addr, inst.depth)
          );

          request_read_bank_for_tdma(self.vbank_id, 0u64, self.depth, self.rob_id);

          model_record!(
            self,
            services,
            "read_bank",
            format!("id={}, count={}", self.vbank_id, self.depth)
          );

          self.state = TdmaStorerState::Wait;
          self.until_next_event = 1.0;
        }
      },
      TdmaStorerState::Wait => {
        // Wait state: until_next_event should always be 1.0
        // This state waits for external event (read_bank_resp_port)
        self.until_next_event = 1.0;
      },
      TdmaStorerState::Active => {
        self.state = TdmaStorerState::Complete;
        self.until_next_event = 1.0;
      },
      TdmaStorerState::Complete => {
        messages.push(ModelMessage {
          content: serde_json::to_string(&self.rob_id).map_err(|_| SimulationError::InvalidModelState)?,
          port_name: self.commit_to_rob_port.clone(),
        });

        model_record!(self, services, "commit_mvout", format!("rob_id={}", self.rob_id));

        MVOUT_INST_CAN_ISSUE.store(true, Ordering::Relaxed);
        self.state = TdmaStorerState::Idle;
        self.until_next_event = INFINITY;
      },
    }

    Ok(messages)
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    if self.state == TdmaStorerState::Idle && MVOUT_INST_DATA.lock().unwrap().is_some() {
      return 0.0;
    }
    if self.state == TdmaStorerState::Wait {
      return 1.0;
    }
    self.until_next_event
  }
}

impl Reportable for TdmaStorer {
  fn status(&self) -> String {
    format!("state={:?}", self.state)
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.records
  }
}

impl ReportableModel for TdmaStorer {}

impl SerializableModel for TdmaStorer {
  fn get_type(&self) -> &'static str {
    "TdmaStorer"
  }
}

/// ------------------------------------------------------------
/// --- Helper Functions ---
/// ------------------------------------------------------------
fn decode_inst(xs1: u64, xs2: u64) -> (u64, u64, u64, u64) {
  let base_dram_addr = (xs1 & 0xffffffff) as u64;
  let stride = ((xs2 >> 24) & 0x3ff) as u64;
  let depth = ((xs2 >> 8) & 0xffff) as u64;
  let vbank_id = (xs2 & 0xff) as u64;
  (base_dram_addr, stride, depth, vbank_id)
}

pub fn set_dma_write_handler(handler: Arc<Mutex<crate::simulator::server::socket::DmaWriteHandler>>) {
  *DMA_WRITE_HANDLER.lock().unwrap() = Some(handler);
}

/// Receive MVOUT instruction (called by RS or other modules)
/// Caller should check MVOUT_INST_CAN_ISSUE before calling this function
pub fn receive_mvout_inst(xs1: u64, xs2: u64, rob_id: u64) {
  let (base_dram_addr, stride, depth, vbank_id) = decode_inst(xs1, xs2);

  // Set instruction data
  *MVOUT_INST_DATA.lock().unwrap() = Some(MvoutInstData {
    base_dram_addr,
    stride,
    depth,
    vbank_id,
    rob_id,
  });

  // Mark as busy
  MVOUT_INST_CAN_ISSUE.store(false, Ordering::Relaxed);
}

fn dma_write_dram(dram_addr: u64, data: u128) {
  let handler_opt = DMA_WRITE_HANDLER.lock().unwrap();
  if let Some(handler) = handler_opt.as_ref() {
    let mut h = handler.lock().unwrap();
    let _ = h.write(dram_addr, data, 16);
  }
}
