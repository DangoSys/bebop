use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tdma {
  port_in: String,
  port_out: String,
  until_next_event: f64,
  records: Vec<ModelRecord>,
}

impl Tdma {
  pub fn new(port_in: String, port_out: String) -> Self {
    Self {
      port_in,
      port_out,
      until_next_event: INFINITY,
      records: Vec::new(),
    }
  }
}

impl DevsModel for Tdma {
  fn events_ext(&mut self, _incoming_message: &ModelMessage, _services: &mut Services) -> Result<(), SimulationError> {
    Ok(())
  }

  fn events_int(&mut self, _services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    Ok(Vec::new())
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
