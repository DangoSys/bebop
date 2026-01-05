use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::collections::HashMap;
use std::f64::INFINITY;

#[derive(Clone)]
pub struct Rob {
  until_next_event: f64,
  num_entries: u32,
  max_entries: u32,
  entries: HashMap<u32, (u32, u64, u64, u8)>,
}

impl Rob {
  pub fn new() -> Self {
    Self {
      until_next_event: 1.0,
      num_entries: 0,
      max_entries: 64,
      entries: HashMap::new(),
    }
  }
}

impl DevsModel for Rob {
  fn events_ext(&mut self, msg_input: &ModelMessage, _services: &mut Services) -> Result<(), SimulationError> {
    self.until_next_event = 0.5;
    Ok(())
  }

  fn events_int(&mut self, _services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    self.until_next_event = 1.0;
    Ok(vec![])
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
    "Rob".to_string()
  }

  fn records(&self) -> &Vec<ModelRecord> {
    static EMPTY: Vec<ModelRecord> = Vec::new();
    &EMPTY
  }
}

impl ReportableModel for Rob {}

impl SerializableModel for Rob {
  fn get_type(&self) -> &'static str {
    "Rob"
  }
}
