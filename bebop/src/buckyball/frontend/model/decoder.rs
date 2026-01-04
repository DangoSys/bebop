use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

#[derive(Clone)]
pub struct Decoder {
  until_next_event: f64,
}

impl Decoder {
  pub fn new() -> Self {
    Self {
      until_next_event: INFINITY,
    }
  }

  pub fn decode(&self, inst: usize) -> ModelMessage {
    let msg = ModelMessage {
      port_name: "decoder_rob".to_string(),
      content: inst.to_string(),
    };
    msg
  }
}

impl DevsModel for Decoder {
  fn events_ext(&mut self, msg_input: &ModelMessage, _services: &mut Services) -> Result<(), SimulationError> {
    if msg_input.port_name == "frontend_decoder" {
      if let Ok(inst) = msg_input.content.parse::<usize>() {
        println!("Decoder: receive instruction {}", inst);
        self.until_next_event = 1.0;
      }
    }
    Ok(())
  }

  fn events_int(&mut self, _services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut msg_output = Vec::new();
    self.until_next_event = INFINITY;
    Ok(msg_output)
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    self.until_next_event
  }
}

impl Reportable for Decoder {
  fn status(&self) -> String {
    "Decoder".to_string()
  }

  fn records(&self) -> &Vec<ModelRecord> {
    static EMPTY: Vec<ModelRecord> = Vec::new();
    &EMPTY
  }
}

impl ReportableModel for Decoder {}

impl SerializableModel for Decoder {
  fn get_type(&self) -> &'static str {
    "Decoder"
  }
}
