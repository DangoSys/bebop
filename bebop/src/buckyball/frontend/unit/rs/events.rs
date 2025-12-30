use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

use crate::buckyball::frontend::unit::rob::bundles::decoder_rob::DecodedInstruction;
use crate::{log_backward, log_forward};
use bebop_lib::ack_msg::AckMessage;
use bebop_lib::msg::{create_message, receive_message};

/// Reservation Station - 接收 ROB 指令并分发到不同 domain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rs {
  // PortsIn 字段
  from_rob: String,
  // PortsOut 字段
  to_memdomain: String,
  to_balldomain: String,
  ack_to_rob: String, // 发送 ACK/NACK 到 ROB
  // State 字段
  events: Vec<RsEvent>,
  until_next_event: f64,
  records: Vec<ModelRecord>,
  busy: bool, // 是否正在处理指令
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum RsEvent {
  Issue(DecodedInstruction), // 发射指令到对应 domain
  SendAck,                   // 发送 ACK
  SendNack(String),          // 发送 NACK，携带原因
}

impl Rs {
  pub fn new() -> Self {
    Self {
      from_rob: "rob_rs".to_string(),
      to_memdomain: "rs_memdomain".to_string(),
      to_balldomain: "rs_balldomain".to_string(),
      ack_to_rob: "rs_rob_ack".to_string(),
      events: Vec::new(),
      until_next_event: INFINITY,
      records: Vec::new(),
      busy: false,
    }
  }
}

impl DevsModel for Rs {
  fn events_ext(&mut self, msg_input: &ModelMessage, _services: &mut Services) -> Result<(), SimulationError> {
    if let Ok(decoded_inst) = receive_message::<DecodedInstruction>(msg_input, &self.from_rob) {
      log_forward!("RS: funct={}, domain={}", decoded_inst.funct, decoded_inst.domain_id);

      // 检查是否 busy
      if self.busy {
        // 拒绝，发送 NACK
        log_backward!("RS: busy, reject funct={}", decoded_inst.funct);
        self.events.push(RsEvent::SendNack("busy".to_string()));
        self.until_next_event = 0.1; // 立即响应
      } else {
        // 接受，发送 ACK 并处理
        self.busy = true;
        self.events.push(RsEvent::SendAck);
        self.events.push(RsEvent::Issue(decoded_inst));
        self.until_next_event = 0.5; // 0.5 cycle 后发射
      }
    }
    Ok(())
  }

  fn events_int(&mut self, _services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut msg_output = Vec::new();

    for event in self.events.drain(..) {
      match event {
        RsEvent::SendAck => {
          // 发送 ACK 到 ROB
          msg_output.push(create_message(&AckMessage::ack(), &self.ack_to_rob)?);
          log_backward!("RS: send ACK to ROB");
        },
        RsEvent::SendNack(reason) => {
          // 发送 NACK 到 ROB（RS 不知道 retry_count，传 0）
          msg_output.push(create_message(&AckMessage::nack(&reason, 0), &self.ack_to_rob)?);
          log_backward!("RS: send NACK to ROB ({})", reason);
        },
        RsEvent::Issue(inst) => {
          // 根据 domain_id 分发指令
          let (port, domain_name) = match inst.domain_id {
            1 => (&self.to_memdomain, "memdomain"),
            2 => (&self.to_balldomain, "balldomain"),
            _ => {
              log_backward!("RS: unknown domain_id={}, dropped", inst.domain_id);
              self.busy = false; // 释放 busy
              continue;
            },
          };

          msg_output.push(create_message(&inst, port)?);

          log_backward!("RS: issue funct={} to {}", inst.funct, domain_name);
          self.busy = false; // 发射完成，释放 busy
        },
      }
    }

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

impl Reportable for Rs {
  fn status(&self) -> String {
    String::new()
  }

  fn records(&self) -> &Vec<ModelRecord> {
    static EMPTY: Vec<ModelRecord> = Vec::new();
    &EMPTY
  }
}

impl ReportableModel for Rs {}

impl SerializableModel for Rs {
  fn get_type(&self) -> &'static str {
    "Rs"
  }
}
