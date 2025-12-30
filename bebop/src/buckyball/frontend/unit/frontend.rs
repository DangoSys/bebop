use crate::buckyball::frontend::unit::decoder::events::Decoder;
use crate::buckyball::frontend::unit::rob::events::Rob;
use sim::models::{Coupled, ExternalInputCoupling, ExternalOutputCoupling, InternalCoupling, Model};

/// Frontend module - hierarchical model, containing Decoder and ROB
#[derive(Clone)]
pub struct Frontend {
  coupled: Coupled,
}

impl Frontend {
  pub fn new() -> Self {
    // create internal submodules
    let decoder = Model::new(String::from("decoder"), Box::new(Decoder::new()));

    let rob = Model::new(String::from("rob"), Box::new(Rob::new()));

    // define external ports
    let ports_in = vec![String::from("rocc_frontend")];
    let ports_out = vec![String::from("frontend_balldomain")];

    // internal components
    let components = vec![decoder, rob];

    // external input coupling: Frontend.rocc_frontend -> Decoder.frontend_decoder
    let external_input_couplings = vec![ExternalInputCoupling {
      target_id: String::from("decoder"),
      source_port: String::from("rocc_frontend"),
      target_port: String::from("frontend_decoder"),
    }];

    // external output coupling: ROB.rob_balldomain -> Frontend.frontend_balldomain
    let external_output_couplings = vec![ExternalOutputCoupling {
      source_id: String::from("rob"),
      source_port: String::from("rob_balldomain"),
      target_port: String::from("frontend_balldomain"),
    }];

    // internal coupling: Decoder.decoder_rob -> ROB.decoder_rob
    let internal_couplings = vec![InternalCoupling {
      source_id: String::from("decoder"),
      target_id: String::from("rob"),
      source_port: String::from("decoder_rob"),
      target_port: String::from("decoder_rob"),
    }];

    // create Coupled model
    let coupled = Coupled::new(
      ports_in,
      ports_out,
      components,
      external_input_couplings,
      external_output_couplings,
      internal_couplings,
    );

    Self { coupled }
  }
}

use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;

impl DevsModel for Frontend {
  fn events_ext(&mut self, msg_input: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    self.coupled.events_ext(msg_input, services)
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    self.coupled.events_int(services)
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.coupled.time_advance(time_delta)
  }

  fn until_next_event(&self) -> f64 {
    self.coupled.until_next_event()
  }
}

impl Reportable for Frontend {
  fn status(&self) -> String {
    String::new()
  }

  fn records(&self) -> &Vec<ModelRecord> {
    static EMPTY: Vec<ModelRecord> = Vec::new();
    &EMPTY
  }
}

impl ReportableModel for Frontend {}

impl SerializableModel for Frontend {
  fn get_type(&self) -> &'static str {
    "Frontend"
  }
}
