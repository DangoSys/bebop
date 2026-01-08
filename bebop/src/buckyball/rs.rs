use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rs {
  port_in: String,
  port_out: String,
  until_next_event: f64,
  current_inst: Option<String>,
  records: Vec<ModelRecord>,
}

impl Rs {
  pub fn new(port_in: String, port_out: String) -> Self {
    Self {
      port_in,
      port_out,
      until_next_event: INFINITY,
      current_inst: None,
      records: Vec::new(),
    }
  }
}

impl DevsModel for Rs {
  fn events_ext(&mut self, incoming_message: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    if self.current_inst.is_some() {
      println!("[Rs] ERROR: Already processing instruction, cannot accept new one");
      return Err(SimulationError::InvalidModelState);
    }

    println!(
      "[Rs] events_ext: received instruction at t={:.1}: {}",
      services.global_time(),
      incoming_message.content
    );
    self.current_inst = Some(incoming_message.content.clone());
    self.until_next_event = 1.0;

    self.records.push(ModelRecord {
      time: services.global_time(),
      action: "dispatch".to_string(),
      subject: incoming_message.content.clone(),
    });

    Ok(())
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    if let Some(inst) = self.current_inst.take() {
      self.until_next_event = INFINITY;

      println!(
        "[Rs] events_int: issued instruction at t={:.1}: {}",
        services.global_time(),
        inst
      );
      self.records.push(ModelRecord {
        time: services.global_time(),
        action: "issued".to_string(),
        subject: inst.clone(),
      });

      Ok(vec![ModelMessage {
        content: inst,
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

impl Reportable for Rs {
  fn status(&self) -> String {
    if self.current_inst.is_some() {
      "busy".to_string()
    } else {
      "idle".to_string()
    }
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.records
  }
}

impl ReportableModel for Rs {}

impl SerializableModel for Rs {
  fn get_type(&self) -> &'static str {
    "Rs"
  }
}
