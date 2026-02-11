use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::bmt::get_pbank_ids;
use super::scoreboard;
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

static TDMA_LOADER_STATE: Mutex<TdmaLoaderState> = Mutex::new(TdmaLoaderState::Idle);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum TdmaLoaderState {
  Idle,
  Wait,     // Waiting for DRAM read response
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
  fn events_ext(&mut self, incoming_message: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    // Receive write completion response (if any)
    // For now, we assume write is accepted when request is sent
    // This can be extended if write response port is added
    if self.state == TdmaLoaderState::Wait {
      // Write request has been accepted, move to Active
      self.state = TdmaLoaderState::Active;
      *TDMA_LOADER_STATE.lock().unwrap() = TdmaLoaderState::Active;
      self.until_next_event = 0.0;
    }
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

          // Reserve write request in scoreboard before sending (so read requests can detect dependency)
          let pbank_id = if let Some(pbank_ids) = get_pbank_ids(inst.vbank_id) {
            if pbank_ids.is_empty() {
              inst.vbank_id
            } else {
              pbank_ids[0]
            }
          } else {
            inst.vbank_id
          };
          scoreboard::reserve_write_request(inst.rob_id, pbank_id);

          self.state = TdmaLoaderState::Wait;
          *TDMA_LOADER_STATE.lock().unwrap() = TdmaLoaderState::Wait;
          self.until_next_event = 1.0;
        } else {
          self.until_next_event = INFINITY;
        }
      },
      TdmaLoaderState::Wait => {
        // Wait state: keep sending write request to mem_ctrl
          // Read DRAM data and send write request
          let mut data_u128 = Vec::new();
          for i in 0..self.depth {
            // 当stride=0时，使用默认步长1，避免所有数据都从同一个地址读取
            let stride = if self.stride == 0 { 1 } else { self.stride };
            // 每次读取16字节数据，步长16
            let dram_addr = self.base_dram_addr + i * 16 * stride;
            let (data_lo, data_hi) = dma_read_dram(dram_addr);
            let data_128 = (data_hi as u128) << 64 | (data_lo as u128);
            data_u128.push(data_128);
          }

          let request = (self.rob_id, self.vbank_id, 0u64, data_u128);
        match serde_json::to_string(&request) {
          Ok(content) => {
            messages.push(ModelMessage {
              content,
              port_name: self.write_bank_req_port.clone(),
            });

            model_record!(
              self,
              services,
              "write_bank",
              format!("id={}, count={}", self.vbank_id, self.depth)
            );
          },
          Err(e) => {
            println!("[ERROR] Failed to serialize TDMA write request: {:?}, skipping", e);
            // Mark as completed to avoid blocking
            self.state = TdmaLoaderState::Complete;
            *TDMA_LOADER_STATE.lock().unwrap() = TdmaLoaderState::Complete;
            self.until_next_event = 0.0;
            return Ok(messages);
          }
        }

        // 直接转换到Active状态，不等待MemController的响应
        // 因为MemController的设计是同步处理写请求的
        self.state = TdmaLoaderState::Active;
        *TDMA_LOADER_STATE.lock().unwrap() = TdmaLoaderState::Active;
        self.until_next_event = 0.0;
      },
      TdmaLoaderState::Active => {
        // Write request has been accepted, now wait for transfer latency
        self.until_next_event = self.transfer_latency * self.depth as f64;
        self.state = TdmaLoaderState::Complete;
        *TDMA_LOADER_STATE.lock().unwrap() = TdmaLoaderState::Complete;
      },
      TdmaLoaderState::Complete => {
        messages.push(ModelMessage {
          content: serde_json::to_string(&self.rob_id).map_err(|_| SimulationError::InvalidModelState)?,
          port_name: self.commit_to_rob_port.clone(),
        });

        model_record!(self, services, "commit_mvin", format!("rob_id={}", self.rob_id));

        MVIN_INST_CAN_ISSUE.store(true, Ordering::Relaxed);
        self.state = TdmaLoaderState::Idle;
        *TDMA_LOADER_STATE.lock().unwrap() = TdmaLoaderState::Idle;
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
  let base_dram_addr = xs1;  // 使用完整的64位地址
  // 根据bb_mvin宏的定义解析参数：bank_id(5位) | depth(10位) | stride(19位)
  let vbank_id = (xs2 & 0x1f) as u64;  // 低5位
  let depth = ((xs2 >> 5) & 0x3ff) as u64;  // 中间10位
  let stride = ((xs2 >> 15) & 0x7ffff) as u64;  // 高19位
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
    // 直接使用DmaReadResp的原始数据结构，避免数据转换错误
    // 首先发送读取请求
    if h.send_read_request(dram_addr, 16).is_ok() {
      // 然后接收响应，获取原始的data_lo和data_hi
      match h.recv_read_response() {
        Ok(data) => {
          // 正确拆分u128为两个u64
          let data_lo = data as u64;
          let data_hi = (data >> 64) as u64;
          (data_lo, data_hi)
        },
        Err(_) => {
          (0, 0)
        }
      }
    } else {
      (0, 0)
    }
  } else {
    (0, 0)
  }
}

pub fn is_tdma_loader_idle() -> bool {
  *TDMA_LOADER_STATE.lock().unwrap() == TdmaLoaderState::Idle
}
