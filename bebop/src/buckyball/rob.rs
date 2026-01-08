use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use std::sync::{Arc, Mutex};

pub static ROB_READY_TO_RECEIVE: Mutex<bool> = Mutex::new(true);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rob {
  capacity: usize,
  port_in: String,
  port_out: String,
  queue: Vec<String>,
  until_next_event: f64,
  records: Vec<ModelRecord>,
}

impl Rob {
  pub fn new(capacity: usize, port_in: String, port_out: String) -> Self {
    *ROB_READY_TO_RECEIVE.lock().unwrap() = true;
    Self {
      capacity,
      port_in,
      port_out,
      queue: Vec::new(),
      until_next_event: INFINITY,
      records: Vec::new(),
    }
  }
}

impl DevsModel for Rob {
  fn events_ext(&mut self, incoming_message: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    let can_receive = self.queue.len() < self.capacity;
    *ROB_READY_TO_RECEIVE.lock().unwrap() = can_receive;

    if !can_receive {
      println!("[Rob] ERROR: Queue full ({}/{})", self.queue.len(), self.capacity);
      return Err(SimulationError::InvalidModelState);
    }

    println!(
      "[Rob] events_ext: received instruction at t={:.1}, queue={}/{}: {}",
      services.global_time(),
      self.queue.len(),
      self.capacity,
      incoming_message.content
    );
    self.queue.push(incoming_message.content.clone());
    self.until_next_event = 1.0;

    *ROB_READY_TO_RECEIVE.lock().unwrap() = self.queue.len() < self.capacity;

    self.records.push(ModelRecord {
      time: services.global_time(),
      action: "receive".to_string(),
      subject: incoming_message.content.clone(),
    });

    Ok(())
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    if self.queue.is_empty() {
      self.until_next_event = INFINITY;
      *ROB_READY_TO_RECEIVE.lock().unwrap() = true;
      return Ok(Vec::new());
    }

    let inst = self.queue.remove(0);
    println!(
      "[Rob] events_int: dispatched instruction at t={:.1}, remaining={}: {}",
      services.global_time(),
      self.queue.len(),
      inst
    );

    *ROB_READY_TO_RECEIVE.lock().unwrap() = self.queue.len() < self.capacity;

    self.records.push(ModelRecord {
      time: services.global_time(),
      action: "dispatch".to_string(),
      subject: inst.clone(),
    });

    if self.queue.is_empty() {
      self.until_next_event = INFINITY;
    } else {
      self.until_next_event = 1.0;
    }

    Ok(vec![ModelMessage {
      content: inst,
      port_name: self.port_out.clone(),
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
    format!("{}/{}", self.queue.len(), self.capacity)
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
