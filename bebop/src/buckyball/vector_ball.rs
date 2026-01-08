use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorBall {
  port_in: String,
  port_out: String,
  latency: f64,
  busy: bool,
  until_next_event: f64,
  current_inst: Option<String>,
  records: Vec<ModelRecord>,
}

impl VectorBall {
  pub fn new(port_in: String, port_out: String, latency: f64) -> Self {
    Self {
      port_in,
      port_out,
      latency,
      busy: false,
      until_next_event: INFINITY,
      current_inst: None,
      records: Vec::new(),
    }
  }
}

impl DevsModel for VectorBall {
  fn events_ext(&mut self, incoming_message: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    if self.busy {
      println!("[VectorBall] ERROR: Already busy, cannot accept new instruction");
      return Err(SimulationError::InvalidModelState);
    }

    println!(
      "[VectorBall] events_ext: received instruction at t={:.1}, latency={:.1}: {}",
      services.global_time(),
      self.latency,
      incoming_message.content
    );
    self.busy = true;
    self.current_inst = Some(incoming_message.content.clone());
    self.until_next_event = self.latency;

    self.records.push(ModelRecord {
      time: services.global_time(),
      action: "start".to_string(),
      subject: incoming_message.content.clone(),
    });

    Ok(())
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    if let Some(inst) = self.current_inst.take() {
      self.busy = false;
      self.until_next_event = INFINITY;

      println!(
        "[VectorBall] events_int: completed instruction at t={:.1}: {}",
        services.global_time(),
        inst
      );
      self.records.push(ModelRecord {
        time: services.global_time(),
        action: "complete".to_string(),
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

impl Reportable for VectorBall {
  fn status(&self) -> String {
    if self.busy { "busy" } else { "idle" }.to_string()
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.records
  }
}

impl ReportableModel for VectorBall {}

impl SerializableModel for VectorBall {
  fn get_type(&self) -> &'static str {
    "VectorBall"
  }
}
