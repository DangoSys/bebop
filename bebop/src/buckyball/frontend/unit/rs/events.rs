use serde::{Deserialize, Serialize};
use sim::simulator::Services;
use sim::models::{ModelRecord, ModelMessage};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

use crate::{log_forward, log_backward};
use crate::buckyball::frontend::unit::rob::bundles::decoder_rob::DecodedInstruction;
use bebop_lib::ack_msg::AckMessage;

/// Reservation Station - 接收 ROB 指令并分发到不同 domain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rs {
  ports_in: PortsIn,
  ports_out: PortsOut,
  state: State,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortsIn {
  from_rob: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortsOut {
  to_memdomain: String,
  to_balldomain: String,
  ack_to_rob: String,  // 发送 ACK/NACK 到 ROB
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
  events: Vec<RsEvent>,
  until_next_event: f64,
  records: Vec<ModelRecord>,
  busy: bool,  // 是否正在处理指令
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum RsEvent {
  Issue(DecodedInstruction),  // 发射指令到对应 domain
  SendAck,                    // 发送 ACK
  SendNack(String),           // 发送 NACK，携带原因
}

impl Rs {
  pub fn new() -> Self {
    Self {
      ports_in: PortsIn {
        from_rob: "rob_rs".to_string(),
      },
      ports_out: PortsOut {
        to_memdomain: "rs_memdomain".to_string(),
        to_balldomain: "rs_balldomain".to_string(),
        ack_to_rob: "rs_rob_ack".to_string(),
      },
      state: State {
        events: Vec::new(),
        until_next_event: INFINITY,
        records: Vec::new(),
        busy: false,
      },
    }
  }
}

impl DevsModel for Rs {
  fn events_ext(
    &mut self,
    msg_input: &ModelMessage,
    _services: &mut Services,
  ) -> Result<(), SimulationError> {
    if msg_input.port_name == self.ports_in.from_rob {
      let decoded_inst: DecodedInstruction = serde_json::from_str(&msg_input.content)
        .expect("Failed to deserialize DecodedInstruction");
      
      log_forward!("RS: funct={}, domain={}", decoded_inst.funct, decoded_inst.domain_id);
      
      // 检查是否 busy
      if self.state.busy {
        // 拒绝，发送 NACK
        log_backward!("RS: busy, reject funct={}", decoded_inst.funct);
        self.state.events.push(RsEvent::SendNack("busy".to_string()));
        self.state.until_next_event = 0.1;  // 立即响应
      } else {
        // 接受，发送 ACK 并处理
        self.state.busy = true;
        self.state.events.push(RsEvent::SendAck);
        self.state.events.push(RsEvent::Issue(decoded_inst));
        self.state.until_next_event = 0.5;  // 0.5 cycle 后发射
      }
    }
    Ok(())
  }

  fn events_int(
    &mut self,
    _services: &mut Services,
  ) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut msg_output = Vec::new();
    
    for event in self.state.events.drain(..) {
      match event {
        RsEvent::SendAck => {
          // 发送 ACK 到 ROB
          msg_output.push(ModelMessage {
            port_name: self.ports_out.ack_to_rob.clone(),
            content: serde_json::to_string(&AckMessage::ack())
              .expect("Failed to serialize AckMessage"),
          });
          log_backward!("RS: send ACK to ROB");
        }
        RsEvent::SendNack(reason) => {
          // 发送 NACK 到 ROB（RS 不知道 retry_count，传 0）
          msg_output.push(ModelMessage {
            port_name: self.ports_out.ack_to_rob.clone(),
            content: serde_json::to_string(&AckMessage::nack(&reason, 0))
              .expect("Failed to serialize AckMessage"),
          });
          log_backward!("RS: send NACK to ROB ({})", reason);
        }
        RsEvent::Issue(inst) => {
          // 根据 domain_id 分发指令
          let (port, domain_name) = match inst.domain_id {
            1 => (&self.ports_out.to_memdomain, "memdomain"),
            2 => (&self.ports_out.to_balldomain, "balldomain"),
            _ => {
              log_backward!("RS: unknown domain_id={}, dropped", inst.domain_id);
              self.state.busy = false;  // 释放 busy
              continue;
            }
          };
          
          msg_output.push(ModelMessage {
            port_name: port.clone(),
            content: serde_json::to_string(&inst)
              .expect("Failed to serialize DecodedInstruction"),
          });
          
          log_backward!("RS: issue funct={} to {}", inst.funct, domain_name);
          self.state.busy = false;  // 发射完成，释放 busy
        }
      }
    }
    
    self.state.until_next_event = INFINITY;
    Ok(msg_output)
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.state.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    self.state.until_next_event
  }
}

impl Reportable for Rs {
  fn status(&self) -> String {
    format!("RS - Events: {}", self.state.events.len())
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.state.records
  }
}

impl ReportableModel for Rs {}

impl SerializableModel for Rs {
  fn get_type(&self) -> &'static str {
    "Rs"
  }
}
