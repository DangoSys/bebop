use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

#[derive(Clone)]
pub struct CycleModel {
  until_next_event: f64,
}

impl CycleModel {
  pub fn new() -> Self {
    Self {
      until_next_event: INFINITY,
    }
  }
}

impl DevsModel for CycleModel {
  fn events_ext(&mut self, msg_input: &ModelMessage, _services: &mut Services) -> Result<(), SimulationError> {
    self.until_next_event = 1.0;
    Ok(())
  }

  fn events_int(&mut self, _services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    self.until_next_event = INFINITY;

    let resp = ModelMessage {
      port_name: "output".to_string(),
      content: "finish".to_string(),
    };

    Ok(vec![resp])
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    self.until_next_event
  }
}

impl Reportable for CycleModel {
  fn status(&self) -> String {
    "CycleModel".to_string()
  }

  fn records(&self) -> &Vec<ModelRecord> {
    static EMPTY: Vec<ModelRecord> = Vec::new();
    &EMPTY
  }
}

impl ReportableModel for CycleModel {}

impl SerializableModel for CycleModel {
  fn get_type(&self) -> &'static str {
    "CycleModel"
  }
}
