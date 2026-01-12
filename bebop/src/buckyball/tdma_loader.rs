use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::model_record;
use crate::simulator::server::socket::DmaReadHandler;

static DMA_READ_HANDLER: Mutex<Option<Arc<Mutex<DmaReadHandler>>>> = Mutex::new(None);

pub static MVIN_INST_CAN_ISSUE: AtomicBool = AtomicBool::new(true);

struct MvinInstData {
  base_dram_addr: u64,
  stride: u64,
  depth: u64,
  vbank_id: u64,
  rob_id: u64,
}

static MVIN_INST_DATA: Mutex<Option<MvinInstData>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum TdmaLoaderState {
  Idle,
  Active,   // DRAM -> Bank batch transfer in progress
  Complete, // Batch transfer complete
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TdmaLoader {
  write_bank_req_port: String,
  commit_to_rob_port: String,

  state: TdmaLoaderState,

  // MVIN state (DRAM -> Bank)
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

impl TdmaLoader {
  pub fn new(write_bank_req_port: String, commit_to_rob_port: String) -> Self {
    MVIN_INST_CAN_ISSUE.store(true, Ordering::Relaxed);
    *MVIN_INST_DATA.lock().unwrap() = None;
    Self {
      write_bank_req_port,
      commit_to_rob_port,
      until_next_event: INFINITY,
      records: Vec::new(),
      state: TdmaLoaderState::Idle,
      base_dram_addr: 0,
      stride: 0,
      depth: 0,
      vbank_id: 0,
      rob_id: 0,
      transfer_latency: 1.0,
    }
  }
}

impl DevsModel for TdmaLoader {
  fn events_ext(&mut self, _incoming_message: &ModelMessage, _services: &mut Services) -> Result<(), SimulationError> {
    Ok(())
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut messages = Vec::new();

    match self.state {
      TdmaLoaderState::Idle => {
        if let Some(inst) = MVIN_INST_DATA.lock().unwrap().take() {
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
          self.until_next_event = self.transfer_latency * self.depth as f64;
          self.state = TdmaLoaderState::Active;
        } else {
          self.until_next_event = INFINITY;
        }
      },
      TdmaLoaderState::Active => {
        let mut data_u64 = Vec::new();
        for i in 0..self.depth {
          let dram_addr = self.base_dram_addr + i * 16 * self.stride;
          let (data_lo, data_hi) = dma_read_dram(dram_addr);
          data_u64.push(data_lo);
          data_u64.push(data_hi);
        }

        let request = (self.vbank_id, 0u64, data_u64);
        messages.push(ModelMessage {
          content: serde_json::to_string(&request).map_err(|_| SimulationError::InvalidModelState)?,
          port_name: self.write_bank_req_port.clone(),
        });

        model_record!(
          self,
          services,
          "write_bank",
          format!("id={}, count={}", self.vbank_id, self.depth)
        );
        self.until_next_event = 1.0;
        self.state = TdmaLoaderState::Complete;
      },
      TdmaLoaderState::Complete => {
        messages.push(ModelMessage {
          content: serde_json::to_string(&self.rob_id).map_err(|_| SimulationError::InvalidModelState)?,
          port_name: self.commit_to_rob_port.clone(),
        });

        model_record!(self, services, "commit_mvin", format!("rob_id={}", self.rob_id));

        MVIN_INST_CAN_ISSUE.store(true, Ordering::Relaxed);
        self.state = TdmaLoaderState::Idle;
        self.until_next_event = INFINITY;
      },
    }

    Ok(messages)
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    if self.state == TdmaLoaderState::Idle && MVIN_INST_DATA.lock().unwrap().is_some() {
      return 0.0;
    }
    self.until_next_event
  }
}

impl Reportable for TdmaLoader {
  fn status(&self) -> String {
    format!("state={:?}", self.state)
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.records
  }
}

impl ReportableModel for TdmaLoader {}

impl SerializableModel for TdmaLoader {
  fn get_type(&self) -> &'static str {
    "TdmaLoader"
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

pub fn set_dma_read_handler(handler: Arc<Mutex<crate::simulator::server::socket::DmaReadHandler>>) {
  *DMA_READ_HANDLER.lock().unwrap() = Some(handler);
}

pub fn receive_mvin_inst(xs1: u64, xs2: u64, rob_id: u64) {
  let (base_dram_addr, stride, depth, vbank_id) = decode_inst(xs1, xs2);

  *MVIN_INST_DATA.lock().unwrap() = Some(MvinInstData {
    base_dram_addr,
    stride,
    depth,
    vbank_id,
    rob_id,
  });

  MVIN_INST_CAN_ISSUE.store(false, Ordering::Relaxed);
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
