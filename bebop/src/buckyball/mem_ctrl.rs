use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use std::sync::Mutex;

use super::bank::{request_read_bank, request_write_bank};

// Read request source tracking (to route responses correctly)
static READ_SOURCE_QUEUE: Mutex<Vec<String>> = Mutex::new(Vec::new()); // FIFO queue matching bank responses

// Read responses to forward
#[derive(Debug, Clone)]
struct ReadResponse {
  source: String,
  data: Vec<u128>,
}

static READ_RESPONSE_QUEUE: Mutex<Vec<ReadResponse>> = Mutex::new(Vec::new());

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemController {
  // Write request ports (multi-cycle)
  tdma_write_req_port: String,
  vball_write_req_port: String,
  bank_write_req_port: String,

  // Read response ports (multi-cycle)
  tdma_read_resp_port: String,
  vball_read_resp_port: String,
  bank_read_resp_port: String,

  until_next_event: f64,
  records: Vec<ModelRecord>,

  // Track pending write requests
  write_request_queue: Vec<(String, u64, u64, Vec<u128>)>, // (source, vbank_id, start_addr, data_vec)
}

impl MemController {
  pub fn new(
    tdma_write_req_port: String,
    vball_write_req_port: String,
    tdma_read_resp_port: String,
    vball_read_resp_port: String,
    bank_write_req_port: String,
    bank_read_resp_port: String,
  ) -> Self {
    READ_SOURCE_QUEUE.lock().unwrap().clear();
    READ_RESPONSE_QUEUE.lock().unwrap().clear();
    Self {
      tdma_write_req_port,
      vball_write_req_port,
      bank_write_req_port,
      tdma_read_resp_port,
      vball_read_resp_port,
      bank_read_resp_port,
      until_next_event: INFINITY,
      records: Vec::new(),
      write_request_queue: Vec::new(),
    }
  }
}

impl DevsModel for MemController {
  fn events_ext(&mut self, incoming_message: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    // Handle write requests from TDMA (multi-cycle)
    if incoming_message.port_name == self.tdma_write_req_port {
      let value: (u64, u64, Vec<u64>) =
        serde_json::from_str(&incoming_message.content).map_err(|_| SimulationError::InvalidModelState)?;

      let vbank_id = value.0;
      let start_addr = value.1;
      let data_u64 = value.2;

      // Convert pairs of u64 to u128
      let mut data_vec = Vec::new();
      for i in (0..data_u64.len()).step_by(2) {
        if i + 1 < data_u64.len() {
          let lo = data_u64[i];
          let hi = data_u64[i + 1];
          data_vec.push((hi as u128) << 64 | (lo as u128));
        }
      }

      self
        .write_request_queue
        .push(("tdma".to_string(), vbank_id, start_addr, data_vec.clone()));

      self.records.push(ModelRecord {
        time: services.global_time(),
        action: "enqueue_tdma_write".to_string(),
        subject: format!("bank={}, addr={}, count={}", vbank_id, start_addr, data_vec.len()),
      });

      self.until_next_event = 1.0;
      return Ok(());
    }

    // Handle write requests from VectorBall (multi-cycle)
    if incoming_message.port_name == self.vball_write_req_port {
      let value: (u64, u64, Vec<u64>) =
        serde_json::from_str(&incoming_message.content).map_err(|_| SimulationError::InvalidModelState)?;

      let vbank_id = value.0;
      let start_addr = value.1;
      let data_u64 = value.2;

      // Convert pairs of u64 to u128
      let mut data_vec = Vec::new();
      for i in (0..data_u64.len()).step_by(2) {
        if i + 1 < data_u64.len() {
          let lo = data_u64[i];
          let hi = data_u64[i + 1];
          data_vec.push((hi as u128) << 64 | (lo as u128));
        }
      }

      self
        .write_request_queue
        .push(("vecball".to_string(), vbank_id, start_addr, data_vec.clone()));

      self.records.push(ModelRecord {
        time: services.global_time(),
        action: "enqueue_vball_write".to_string(),
        subject: format!("bank={}, addr={}, count={}", vbank_id, start_addr, data_vec.len()),
      });

      self.until_next_event = 1.0;
      return Ok(());
    }

    // Handle read responses from Bank - forward to the correct source (multi-cycle)
    if incoming_message.port_name == self.bank_read_resp_port {
      let data_vec: Vec<u128> =
        serde_json::from_str(&incoming_message.content).map_err(|_| SimulationError::InvalidModelState)?;

      // Get source from queue (FIFO)
      if let Some(source) = READ_SOURCE_QUEUE.lock().unwrap().pop() {
        READ_RESPONSE_QUEUE
          .lock()
          .unwrap()
          .push(ReadResponse { source, data: data_vec });
        self.until_next_event = 1.0;
      }
      return Ok(());
    }

    Ok(())
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut messages = Vec::new();

    // Forward read responses
    for resp in READ_RESPONSE_QUEUE.lock().unwrap().drain(..) {
      let response_port = if resp.source == "tdma" {
        self.tdma_read_resp_port.clone()
      } else {
        self.vball_read_resp_port.clone()
      };

      messages.push(ModelMessage {
        content: serde_json::to_string(&resp.data).map_err(|_| SimulationError::InvalidModelState)?,
        port_name: response_port,
      });

      self.records.push(ModelRecord {
        time: services.global_time(),
        action: "forward_read_resp".to_string(),
        subject: format!("to {}", resp.source),
      });
    }

    // Process write requests (forward to bank)
    if !self.write_request_queue.is_empty() {
      let (source, vbank_id, start_addr, data_vec) = self.write_request_queue.remove(0);

      // Convert data_vec to u64 pairs for serialization
      let mut data_u64 = Vec::new();
      for &val in &data_vec {
        data_u64.push((val & 0xFFFFFFFFFFFFFFFF) as u64);
        data_u64.push(((val >> 64) & 0xFFFFFFFFFFFFFFFF) as u64);
      }

      let request = (vbank_id, start_addr, data_u64);
      messages.push(ModelMessage {
        content: serde_json::to_string(&request).map_err(|_| SimulationError::InvalidModelState)?,
        port_name: self.bank_write_req_port.clone(),
      });

      self.records.push(ModelRecord {
        time: services.global_time(),
        action: "forward_write_req".to_string(),
        subject: format!("from {}", source),
      });

      // Write response is single cycle, so no need to track
    }

    // Schedule next event
    if !self.write_request_queue.is_empty() {
      self.until_next_event = 1.0;
    } else {
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

impl Reportable for MemController {
  fn status(&self) -> String {
    format!(
      "write_queue={}, read_sources={}",
      self.write_request_queue.len(),
      READ_SOURCE_QUEUE.lock().unwrap().len()
    )
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.records
  }
}

impl ReportableModel for MemController {}

impl SerializableModel for MemController {
  fn get_type(&self) -> &'static str {
    "MemController"
  }
}

pub fn request_read_bank_for_tdma(vbank_id: u64, start_addr: u64, count: u64) {
  READ_SOURCE_QUEUE.lock().unwrap().push("tdma".to_string());
  request_read_bank(vbank_id, start_addr, count);
}

pub fn request_read_bank_for_vecball(vbank_id: u64, start_addr: u64, count: u64) {
  READ_SOURCE_QUEUE.lock().unwrap().push("vecball".to_string());

  request_read_bank(vbank_id, start_addr, count);
}

pub fn request_write_bank_for_tdma(vbank_id: u64, start_addr: u64, data_vec: Vec<u128>) -> bool {
  request_write_bank(vbank_id, start_addr, data_vec)
}

pub fn request_write_bank_for_vecball(vbank_id: u64, start_addr: u64, data_vec: Vec<u128>) -> bool {
  request_write_bank(vbank_id, start_addr, data_vec)
}
