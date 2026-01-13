use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use std::sync::Mutex;

use super::bank::{request_read_bank, request_write_bank};
use super::bmt::get_pbank_ids;
use super::scoreboard;

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

  // Track pending write requests (ready to process)
  write_request_queue: Vec<(String, String)>, // (source, json_content)
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
    scoreboard::init_scoreboard();
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
      // Parse request: (rob_id, vbank_id, start_addr, data_u64)
      let value: (u64, u64, u64, Vec<u64>) =
        serde_json::from_str(&incoming_message.content).map_err(|_| SimulationError::InvalidModelState)?;
      let rob_id = value.0;
      let vbank_id = value.1;
      let start_addr = value.2;
      let data_count = value.3.len() / 2;

      // Convert vbank_id to pbank_id using BMT
      let pbank_id = if let Some(pbank_ids) = get_pbank_ids(vbank_id) {
        if pbank_ids.is_empty() {
          vbank_id
        } else {
          pbank_ids[0]
        }
      } else {
        vbank_id
      };

      // Check dependency
      if scoreboard::check_dependency(pbank_id, rob_id) {
        // No dependency, can proceed immediately
        self
          .write_request_queue
          .push(("tdma".to_string(), incoming_message.content.clone()));
      } else {
        // Has dependency, add to scoreboard
        scoreboard::add_to_scoreboard(rob_id, pbank_id, "tdma".to_string(), incoming_message.content.clone());
      }

      self.records.push(ModelRecord {
        time: services.global_time(),
        action: "enqueue_tdma_write".to_string(),
        subject: format!(
          "rob_id={}, bank={}, addr={}, count={}",
          rob_id, vbank_id, start_addr, data_count
        ),
      });

      self.until_next_event = 1.0;
      return Ok(());
    }

    // Handle write requests from VectorBall (multi-cycle)
    if incoming_message.port_name == self.vball_write_req_port {
      // Parse request: (rob_id, vbank_id, start_addr, data_u64)
      let value: (u64, u64, u64, Vec<u64>) =
        serde_json::from_str(&incoming_message.content).map_err(|_| SimulationError::InvalidModelState)?;
      let rob_id = value.0;
      let vbank_id = value.1;
      let start_addr = value.2;
      let data_count = value.3.len() / 2;

      // Convert vbank_id to pbank_id using BMT
      let pbank_id = if let Some(pbank_ids) = get_pbank_ids(vbank_id) {
        if pbank_ids.is_empty() {
          vbank_id
        } else {
          pbank_ids[0]
        }
      } else {
        vbank_id
      };

      // Check dependency
      if scoreboard::check_dependency(pbank_id, rob_id) {
        // No dependency, can proceed immediately
        self
          .write_request_queue
          .push(("vecball".to_string(), incoming_message.content.clone()));
      } else {
        // Has dependency, add to scoreboard
        scoreboard::add_to_scoreboard(
          rob_id,
          pbank_id,
          "vecball".to_string(),
          incoming_message.content.clone(),
        );
      }

      self.records.push(ModelRecord {
        time: services.global_time(),
        action: "enqueue_vball_write".to_string(),
        subject: format!(
          "rob_id={}, bank={}, addr={}, count={}",
          rob_id, vbank_id, start_addr, data_count
        ),
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

    // Each cycle, process only one request (either read response or write request)
    // Priority: read response first, then write request

    // Forward one read response if available
    if let Some(resp) = READ_RESPONSE_QUEUE.lock().unwrap().pop() {
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

      // Schedule next event
      if !READ_RESPONSE_QUEUE.lock().unwrap().is_empty()
        || !self.write_request_queue.is_empty()
        || scoreboard::get_pending_count() > 0
      {
        self.until_next_event = 1.0;
      } else {
        self.until_next_event = INFINITY;
      }

      return Ok(messages);
    }

    // Check scoreboard for ready requests (each cycle, unified judgment)
    let ready_request = scoreboard::get_one_ready_request();
    if let Some((rob_id, pbank_id, source, json_content)) = ready_request {
      self.write_request_queue.push((source, json_content));
    }

    // Process one write request if available
    if !self.write_request_queue.is_empty() {
      let (source, json_content) = self.write_request_queue.remove(0);

      // Parse request: (rob_id, vbank_id, start_addr, data_u64)
      let value: (u64, u64, u64, Vec<u64>) =
        serde_json::from_str(&json_content).map_err(|_| SimulationError::InvalidModelState)?;
      let rob_id = value.0;
      let vbank_id = value.1;
      let start_addr = value.2;
      let data_u64 = value.3;

      // Convert vbank_id to pbank_id using BMT
      // Use first pbank_id if vbank maps to multiple pbanks
      let pbank_id = if let Some(pbank_ids) = get_pbank_ids(vbank_id) {
        if pbank_ids.is_empty() {
          vbank_id
        } else {
          pbank_ids[0]
        }
      } else {
        vbank_id
      };

      // Mark as in-flight
      scoreboard::mark_in_flight(pbank_id, rob_id);

      // Re-encode with pbank_id (remove rob_id for bank)
      let request = (pbank_id, start_addr, data_u64);
      let new_content = serde_json::to_string(&request).map_err(|_| SimulationError::InvalidModelState)?;

      messages.push(ModelMessage {
        content: new_content,
        port_name: self.bank_write_req_port.clone(),
      });

      self.records.push(ModelRecord {
        time: services.global_time(),
        action: "forward_write_req".to_string(),
        subject: format!(
          "from {}, rob_id={}, vbank={}->pbank={}",
          source, rob_id, vbank_id, pbank_id
        ),
      });

      // Bank write is synchronous (single cycle), mark as completed immediately
      scoreboard::mark_completed(pbank_id);

      // Check if there are ready read requests that can now proceed (unified judgment each cycle)
      let ready_read = scoreboard::get_one_ready_read_request();
      if let Some((read_rob_id, read_pbank_id, read_start_addr, read_count, read_source)) = ready_read {
        READ_SOURCE_QUEUE.lock().unwrap().push(read_source.clone());
        request_read_bank(read_pbank_id, read_start_addr, read_count);
      }
    }

    // Schedule next event
    // Check if there are ready requests in scoreboard or pending requests
    let pending_count = scoreboard::get_pending_count();
    if !self.write_request_queue.is_empty() || pending_count > 0 || !READ_RESPONSE_QUEUE.lock().unwrap().is_empty() {
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
      "write_queue={}, read_sources={}, scoreboard={}",
      self.write_request_queue.len(),
      READ_SOURCE_QUEUE.lock().unwrap().len(),
      scoreboard::get_pending_count()
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

pub fn request_read_bank_for_tdma(vbank_id: u64, start_addr: u64, count: u64, rob_id: u64) {
  // Convert vbank_id to pbank_id using BMT
  // Use first pbank_id if vbank maps to multiple pbanks
  let pbank_id = if let Some(pbank_ids) = get_pbank_ids(vbank_id) {
    if pbank_ids.is_empty() {
      vbank_id // Fallback to vbank_id
    } else {
      pbank_ids[0]
    }
  } else {
    vbank_id // Fallback to vbank_id
  };

  // Check dependency
  if scoreboard::check_dependency(pbank_id, rob_id) {
    // No dependency, can proceed immediately
    READ_SOURCE_QUEUE.lock().unwrap().push("tdma".to_string());
    request_read_bank(pbank_id, start_addr, count);
  } else {
    // Has dependency, add to read scoreboard
    scoreboard::add_read_to_scoreboard(rob_id, pbank_id, start_addr, count, "tdma".to_string());
  }
}

pub fn request_read_bank_for_vecball(vbank_id: u64, start_addr: u64, count: u64, rob_id: u64) {
  // Convert vbank_id to pbank_id using BMT
  // Use first pbank_id if vbank maps to multiple pbanks
  let pbank_id = if let Some(pbank_ids) = get_pbank_ids(vbank_id) {
    if pbank_ids.is_empty() {
      vbank_id // Fallback to vbank_id
    } else {
      pbank_ids[0]
    }
  } else {
    vbank_id // Fallback to vbank_id
  };

  // Check dependency
  if scoreboard::check_dependency(pbank_id, rob_id) {
    // No dependency, can proceed immediately
    READ_SOURCE_QUEUE.lock().unwrap().push("vecball".to_string());
    request_read_bank(pbank_id, start_addr, count);
  } else {
    // Has dependency, add to read scoreboard
    scoreboard::add_read_to_scoreboard(rob_id, pbank_id, start_addr, count, "vecball".to_string());
  }
}

pub fn request_write_bank_for_tdma(vbank_id: u64, start_addr: u64, data_vec: Vec<u128>) -> bool {
  request_write_bank(vbank_id, start_addr, data_vec)
}

pub fn request_write_bank_for_vecball(vbank_id: u64, start_addr: u64, data_vec: Vec<u128>) -> bool {
  request_write_bank(vbank_id, start_addr, data_vec)
}
