use sim::models::{Model, Coupled, ExternalInputCoupling, ExternalOutputCoupling, InternalCoupling};
use super::{decoder::Decoder, rob::Rob};

/// Frontend模块 - 层次化模型，包含Decoder和ROB
#[derive(Clone)]
pub struct Frontend {
  coupled: Coupled,
}

impl Frontend {
  pub fn new() -> Self {
    // 创建内部子模块
    let decoder = Model::new(
      String::from("decoder"),
      Box::new(Decoder::new()),
    );
    
    let rob = Model::new(
      String::from("rob"),
      Box::new(Rob::new()),
    );
    
    // 定义Frontend的外部端口
    let ports_in = vec![String::from("instruction")];
    let ports_out = vec![String::from("to_compute")];
    
    // 内部组件
    let components = vec![decoder, rob];
    
    // 外部输入映射: Frontend.instruction -> Decoder.instruction
    let external_input_couplings = vec![
      ExternalInputCoupling {
        target_id: String::from("decoder"),
        source_port: String::from("instruction"),
        target_port: String::from("instruction"),
      },
    ];
    
    // 外部输出映射: ROB.to_compute -> Frontend.to_compute
    let external_output_couplings = vec![
      ExternalOutputCoupling {
        source_id: String::from("rob"),
        source_port: String::from("to_compute"),
        target_port: String::from("to_compute"),
      },
    ];
    
    // 内部连接: Decoder.decoded -> ROB.decoded
    let internal_couplings = vec![
      InternalCoupling {
        source_id: String::from("decoder"),
        target_id: String::from("rob"),
        source_port: String::from("decoded"),
        target_port: String::from("decoded"),
      },
    ];
    
    // 创建Coupled模型
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

// 为Frontend实现必要的trait，委托给内部的Coupled
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::simulator::Services;
use sim::models::{ModelRecord, ModelMessage};
use sim::utils::errors::SimulationError;

impl DevsModel for Frontend {
  fn events_ext(
    &mut self,
    incoming_message: &ModelMessage,
    services: &mut Services,
  ) -> Result<(), SimulationError> {
    self.coupled.events_ext(incoming_message, services)
  }

  fn events_int(
    &mut self,
    services: &mut Services,
  ) -> Result<Vec<ModelMessage>, SimulationError> {
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
    format!("Frontend (Coupled: Decoder + ROB)")
  }

  fn records(&self) -> &Vec<ModelRecord> {
    self.coupled.records()
  }
}

impl ReportableModel for Frontend {}

impl SerializableModel for Frontend {
  fn get_type(&self) -> &'static str {
    "Frontend"
  }
}
