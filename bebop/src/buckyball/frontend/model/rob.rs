use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

#[derive(Clone)]
pub struct Rob {
  queue: Vec<usize>,
  until_next_event: f64,
  input_port: String,
}

impl Rob {
  pub fn new() -> Self {
    Self {
      queue: Vec::new(),
      until_next_event: INFINITY,
      input_port: "decoder_rob".to_string(),
    }
  }
}

impl DevsModel for Rob {
  fn events_ext(&mut self, msg_input: &ModelMessage, _services: &mut Services) -> Result<(), SimulationError> {
    if msg_input.port_name == self.input_port {
      if let Ok(inst) = msg_input.content.parse::<usize>() {
        println!("ROB: receive instruction {} (queue size: {})", inst, self.queue.len());
        self.queue.push(inst);
        self.until_next_event = 1.0;
      }
    }
    Ok(())
  }

  fn events_int(&mut self, _services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    if let Some(inst) = self.queue.pop() {
      println!("ROB: pop instruction {} (queue size: {})", inst, self.queue.len());
    }

    if self.queue.is_empty() {
      self.until_next_event = INFINITY;
    } else {
      self.until_next_event = 1.0;
    }

    Ok(Vec::new())
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
    format!("ROB queue size: {}", self.queue.len())
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
