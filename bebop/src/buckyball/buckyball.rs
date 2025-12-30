use sim::models::{Coupled, ExternalInputCoupling, InternalCoupling, Model};

use super::balldomain::balldomain::Balldomain;
use super::frontend::unit::frontend::Frontend;
use super::memdomain::memdomain::Memdomain;

pub struct Buckyball;

impl Buckyball {
  pub fn new() -> Coupled {
    Coupled::new(
      vec!["inject".to_string()],
      vec![],
      vec![
        Model::new("frontend".to_string(), Box::new(Frontend::new())),
        Model::new("compute".to_string(), Box::new(Balldomain::new())),
        Model::new("memory".to_string(), Box::new(Memdomain::new())),
      ],
      vec![ExternalInputCoupling {
        target_id: "frontend".to_string(),
        source_port: "inject".to_string(),
        target_port: "rocc_frontend".to_string(),
      }],
      vec![],
      vec![
        InternalCoupling {
          source_id: "frontend".to_string(),
          target_id: "compute".to_string(),
          source_port: "frontend_balldomain".to_string(),
          target_port: "frontend_balldomain".to_string(),
        },
        InternalCoupling {
          source_id: "compute".to_string(),
          target_id: "memory".to_string(),
          source_port: "balldomain_memdomain".to_string(),
          target_port: "balldomain_memdomain".to_string(),
        },
        InternalCoupling {
          source_id: "memory".to_string(),
          target_id: "compute".to_string(),
          source_port: "memdomain_balldomain".to_string(),
          target_port: "memdomain_balldomain".to_string(),
        },
      ],
    )
  }
}
