use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bank {
  port_in: String,
  port_out: String,
  latency: f64,
  busy: bool,
  until_next_event: f64,
  current_req: Option<String>,
  records: Vec<ModelRecord>,
}

impl Bank {
  pub fn new(port_in: String, port_out: String, latency: f64) -> Self {
    Self {
      port_in,
      port_out,
      latency,
      busy: false,
      until_next_event: INFINITY,
      current_req: None,
      records: Vec::new(),
    }
  }
}

impl DevsModel for Bank {
  fn events_ext(&mut self, incoming_message: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    if self.busy {
      return Err(SimulationError::InvalidModelState);
    }

    self.busy = true;
    self.current_req = Some(incoming_message.content.clone());
    self.until_next_event = self.latency;

    self.records.push(ModelRecord {
      time: services.global_time(),
      action: "read_start".to_string(),
      subject: incoming_message.content.clone(),
    });

    Ok(())
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    if let Some(req) = self.current_req.take() {
      self.busy = false;
      self.until_next_event = INFINITY;

      self.records.push(ModelRecord {
        time: services.global_time(),
        action: "read_complete".to_string(),
        subject: req.clone(),
      });

      Ok(vec![ModelMessage {
        content: req,
        port_name: self.port_out.clone(),
      }])
    } else {
      Ok(Vec::new())
    }
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    self.until_next_event
  }
}

impl Reportable for Bank {
  fn status(&self) -> String {
    if self.busy { "busy" } else { "idle" }.to_string()
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
