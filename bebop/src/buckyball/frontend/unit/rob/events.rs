use bebop_lib::ack_msg::AckMessage;
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

use crate::buckyball::frontend::unit::rob::bundles::decoder_rob::DecodedInstruction;
use crate::buckyball::frontend::unit::rob::unit::dispatch::DispatchPolicy;
use crate::buckyball::frontend::unit::rob::unit::ring_buffer::RingBuffer;
use crate::{log_backward, log_forward};
use bebop_lib::msg::{create_message, receive_message};

#[derive(Debug, Clone)]
pub struct Rob {
  decoded_rob: String,
  rs_ack: String,
  to_rs: String,

  events: Vec<RobEvent>,
  until_next_event: f64,
  buffer: RingBuffer,
  pending_dispatch: Option<(DecodedInstruction, u32)>, // (指令, 重试次数)
}

#[derive(Debug, Clone, PartialEq)]
enum RobEvent {
  EnterRob(DecodedInstruction), // 指令进入 ROB
  CmdCommit(u64),               // CMD 提交事件，携带 Rob ID
}

impl Rob {
  pub fn new() -> Self {
    Self {
      decoded_rob: "decoder_rob".to_string(),
      rs_ack: "rs_rob_ack".to_string(),
      to_rs: "rob_rs".to_string(),
      events: Vec::new(),
      until_next_event: INFINITY,
      buffer: RingBuffer::new(16), // ROB 容量为 16
      pending_dispatch: None,
    }
  }
}

impl DevsModel for Rob {
  fn events_ext(&mut self, msg_input: &ModelMessage, _services: &mut Services) -> Result<(), SimulationError> {
    // -----------------------------------------------------
    // EnterRoB
    // -----------------------------------------------------
    if let Ok(decoded_inst) = receive_message::<DecodedInstruction>(msg_input, &self.decoded_rob) {
      log_forward!("ROB: funct={}, domain={}", decoded_inst.funct, decoded_inst.domain_id);
      self.events.push(RobEvent::EnterRob(decoded_inst.clone()));
      if self.until_next_event == INFINITY {
        self.until_next_event = 1.0;
      }
    } else if let Ok(ack) = receive_message::<AckMessage>(msg_input, &self.rs_ack) {
      // 接收 RS 的 ACK/NACK

      if ack.accepted {
        // RS 接受了，清除 pending
        log_backward!("ROB: RS ACK received");
        self.pending_dispatch = None;
      } else {
        // RS 拒绝了（busy），pending 保持不变，准备重试
        if let Some((inst, retry_count)) = &mut self.pending_dispatch {
          *retry_count += 1;
          log_backward!("ROB: RS NACK ({}), will retry (attempt {})", ack.reason, *retry_count);
        }
        self.until_next_event = 2.0; // 2 cycle 后重试
      }
    }
    Ok(())
  }

  fn events_int(&mut self, _services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut msg_output = Vec::new();
    // -----------------------------------------------------
    // Do retry dispatch to RS
    // -----------------------------------------------------
    if let Some((inst, retry_count)) = &self.pending_dispatch {
      msg_output.push(create_message(inst, &self.to_rs)?);
      log_backward!("ROB: retry dispatch funct={} (attempt {})", inst.funct, retry_count);
      self.until_next_event = INFINITY; // 等待 ACK
      return Ok(msg_output);
    }
    // -----------------------------------------------------
    // Handle new events
    // -----------------------------------------------------
    for event in self.events.drain(..) {
      match event {
        RobEvent::EnterRob(decoded_inst) => {
          if !self.buffer.push_in_rob(decoded_inst.clone()) {
            log_backward!("ROB: buffer full, dropped instruction");
            continue;
          }
          if DispatchPolicy::can_dispatch(&decoded_inst) {
            // 保存到 pending，等待 ACK，初始 retry_count = 0
            self.pending_dispatch = Some((decoded_inst.clone(), 0));
            msg_output.push(create_message(&decoded_inst, &self.to_rs)?);
            log_backward!("ROB: dispatch funct={} to RS", decoded_inst.funct);
          }
        },
        RobEvent::CmdCommit(cmd_id) => {
          log_backward!("ROB: CmdCommit cmd_id={}", cmd_id);
        },
      }
    }

    if !self.buffer.is_empty() {
      if let Some(next_inst) = self.buffer.peek() {
        self.until_next_event = 1.0;
      }
    } else {
      self.until_next_event = INFINITY;
    }

    Ok(msg_output)
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
    String::new()
  }
  fn records(&self) -> &Vec<ModelRecord> {
    static EMPTY: Vec<ModelRecord> = Vec::new();
    &EMPTY
  }
}
impl ReportableModel for Rob {}
impl SerializableModel for Rob {
  fn get_type(&self) -> &'static str {
    "ROB"
  }
}
