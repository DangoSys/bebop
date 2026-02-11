use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use std::sync::Mutex;

use crate::model_record;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SRAM {
  data: Vec<u128>,
}

impl SRAM {
  fn new(depth: u64) -> Self {
    Self {
      data: vec![0; depth as usize],
    }
  }

  fn read_batch(&self, start_addr: u64, count: u64) -> Vec<u128> {
    let mut result = Vec::new();
    for i in 0..count {
      let addr = start_addr + i;
      if addr < self.data.len() as u64 {
        result.push(self.data[addr as usize]);
      } else {
        result.push(0);
      }
    }
    result
  }

  fn write_batch(&mut self, start_addr: u64, data: &[u128]) {
    for (i, &val) in data.iter().enumerate() {
      let addr = start_addr + i as u64;
      if addr < self.data.len() as u64 {
        self.data[addr as usize] = val;
      }
    }
  }
}

// Read response (data is read immediately, but response is sent after latency)
#[derive(Debug, Clone)]
struct ReadResponse {
  data: Vec<u128>,
}

// Global storage for bank data (accessed by function calls)
static BANK_DATA: Mutex<Option<Vec<Vec<u128>>>> = Mutex::new(None);
static READ_RESPONSE_QUEUE: Mutex<Vec<ReadResponse>> = Mutex::new(Vec::new());

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WriteRequest {
  vbank_id: u64,
  start_addr: u64,
  data_vec: Vec<u128>,
  complete_time: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bank {
  depth: u64,
  num_banks: u64,
  banks: Vec<SRAM>,
  write_bank_req_port: String,
  read_bank_resp_port: String,
  latency: f64,
  until_next_event: f64,
  records: Vec<ModelRecord>,
  write_requests: Vec<WriteRequest>, // Only for write requests (multi-cycle)
}

impl Bank {
  pub fn new(
    write_bank_req_port: String,
    read_bank_resp_port: String,
    latency: f64,
    num_banks: u64,
    depth: u64,
  ) -> Self {
    READ_RESPONSE_QUEUE.lock().unwrap().clear();
    let banks = (0..num_banks).map(|_| SRAM::new(depth)).collect::<Vec<_>>();
    let bank_data: Vec<Vec<u128>> = banks.iter().map(|sram| sram.data.clone()).collect();

    *BANK_DATA.lock().unwrap() = Some(bank_data);

    Self {
      depth,
      num_banks,
      banks,
      write_bank_req_port,
      read_bank_resp_port,
      latency,
      until_next_event: INFINITY,
      records: Vec::new(),
      write_requests: Vec::new(),
    }
  }

  pub fn sync_bank_data(&mut self) {
    let mut bank_data = BANK_DATA.lock().unwrap();
    if let Some(ref mut data) = *bank_data {
      for (i, sram) in self.banks.iter().enumerate() {
        if i < data.len() {
          data[i] = sram.data.clone();
        }
      }
    }
  }
}

impl DevsModel for Bank {
  fn events_ext(&mut self, incoming_message: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    if incoming_message.port_name == self.write_bank_req_port {
      match serde_json::from_str::<(u64, u64, Vec<u128>)>(&incoming_message.content) {
        Ok(value) => {
          let vbank_id = value.0;
          let start_addr = value.1;
          let data_vec = value.2;

          if vbank_id < self.banks.len() as u64 {
            self.banks[vbank_id as usize].write_batch(start_addr, &data_vec);
            self.sync_bank_data();

            model_record!(
              self,
              services,
              "write_bank",
              format!("id={}, count={}", vbank_id, data_vec.len())
            );
          }
        },
        Err(_) => {
          // Failed to deserialize write request, skipping this request
        }
      }

      return Ok(());
    }

    Ok(())
  }

  fn events_int(&mut self, _services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut messages = Vec::new();

    let mut ready_responses = Vec::new();
    READ_RESPONSE_QUEUE.lock().unwrap().drain(..).for_each(|resp| {
      ready_responses.push(resp.data);
    });

    for data_vec in ready_responses {
      match serde_json::to_string(&data_vec) {
        Ok(content) => {
          messages.push(ModelMessage {
            content,
            port_name: self.read_bank_resp_port.clone(),
          });
        },
        Err(_) => {
          // Failed to serialize read response, skipping this response
        }
      }
    }

    self.until_next_event = INFINITY;

    Ok(messages)
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    let queue_len = READ_RESPONSE_QUEUE.lock().unwrap().len();
    if queue_len > 0 {
      return 0.0;
    }
    self.until_next_event
  }
}

impl Reportable for Bank {
  fn status(&self) -> String {
    format!("read_responses={}", READ_RESPONSE_QUEUE.lock().unwrap().len())
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.records
  }
}

impl ReportableModel for Bank {}

impl SerializableModel for Bank {
  fn get_type(&self) -> &'static str {
    "Bank"
  }
}

/// ------------------------------------------------------------
/// --- Helper Functions ---
/// ------------------------------------------------------------
pub fn request_read_bank(vbank_id: u64, start_addr: u64, count: u64) {
  let bank_data_opt = BANK_DATA.lock().unwrap();
  if let Some(ref bank_data) = *bank_data_opt {
    if vbank_id < bank_data.len() as u64 {
      let bank = &bank_data[vbank_id as usize];

      let mut data_vec = Vec::new();
      for i in 0..count {
        let addr = start_addr + i;
        if addr < bank.len() as u64 {
          data_vec.push(bank[addr as usize]);
        } else {
          data_vec.push(0);
        }
      }

      READ_RESPONSE_QUEUE
        .lock()
        .unwrap()
        .push(ReadResponse { data: data_vec });
    }
  }
}

pub fn request_write_bank(vbank_id: u64, start_addr: u64, data_vec: Vec<u128>) -> bool {
  let mut bank_data_opt = BANK_DATA.lock().unwrap();
  if let Some(ref mut bank_data) = *bank_data_opt {
    if vbank_id < bank_data.len() as u64 {
      let bank = &mut bank_data[vbank_id as usize];

      for (i, &val) in data_vec.iter().enumerate() {
        let addr = start_addr + i as u64;
        if addr < bank.len() as u64 {
          bank[addr as usize] = val;
        }
      }

      return true;
    }
  }
  false
}

pub fn request_read_bank_for_systolic(vbank_id: u64, start_addr: u64, count: u64, _rob_id: u64) {
  let bank_data_opt = BANK_DATA.lock().unwrap();
  if let Some(ref bank_data) = *bank_data_opt {
    if vbank_id < bank_data.len() as u64 {
      let bank = &bank_data[vbank_id as usize];

      let mut data_vec = Vec::new();
      for i in 0..count {
        let addr = start_addr + i;
        if addr < bank.len() as u64 {
          data_vec.push(bank[addr as usize]);
        } else {
          data_vec.push(0);
        }
      }

      READ_RESPONSE_QUEUE
        .lock()
        .unwrap()
        .push(ReadResponse { data: data_vec });
    }
  }
}
