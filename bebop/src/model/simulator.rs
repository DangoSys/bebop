use super::frontend::Rob;
use sim::models::ModelMessage;
use sim::models::model_trait::DevsModel;
use sim::simulator::Services;
use std::f64::INFINITY;

pub struct Simulator {
  rob: Rob,
  services: Services,
}

impl Simulator {
  pub fn new() -> Self {
    Self {
      rob: Rob::new(),
      services: Services::default(),
    }
  }

  pub fn send_message(&mut self, msg: ModelMessage) {
    let _ = self.rob.events_ext(&msg, &mut self.services);
  }

  pub fn step(&mut self) {
    let until_next_event = self.rob.until_next_event();
    
    if until_next_event < INFINITY {
      self.rob.time_advance(until_next_event);
      self.services.set_global_time(self.services.global_time() + until_next_event);

      if self.rob.until_next_event() <= 0.0 {
        let _ = self.rob.events_int(&mut self.services);
      }
    }
  }

  pub fn rob(&self) -> &Rob {
    &self.rob
  }
}

