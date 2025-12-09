use serde::{Deserialize, Serialize};
use sim::simulator::Services;
use sim::models::{ModelRecord, ModelMessage};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use bebop_lib::ack_msg::AckMessage;

use crate::{log_forward, log_backward};
use crate::buckyball::frontend::unit::rob::bundles::decoder_rob::DecodedInstruction;
use crate::buckyball::frontend::unit::rob::unit::dispatch::DispatchPolicy;
use crate::buckyball::frontend::unit::rob::unit::ring_buffer::RingBuffer;

/// ROB (Reorder Buffer) 模块 - 重排序缓冲区
#[derive(Debug, Clone)]
pub struct Rob {
  ports_in: PortsIn,
  ports_out: PortsOut,
  state: State,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortsIn {
  decoded_rob: String,
  rs_ack: String,  // 接收 RS 的 ACK/NACK
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortsOut {
  to_rs: String,
}

#[derive(Debug, Clone)]
struct State {
  events: Vec<RobEvent>,
  until_next_event: f64,
  buffer: RingBuffer,
  pending_dispatch: Option<(DecodedInstruction, u32)>,  // (指令, 重试次数)
}

#[derive(Debug, Clone, PartialEq)]
enum RobEvent {
  EnterRob(DecodedInstruction),  // 指令进入 ROB
  CmdCommit(u64),                // CMD 提交事件，携带 Rob ID 
}

impl Rob {
  pub fn new() -> Self {
    Self {
      ports_in: PortsIn {
        decoded_rob: "decoder_rob".to_string(),
        rs_ack: "rs_rob_ack".to_string(),
      },
      ports_out: PortsOut {
        to_rs: "rob_rs".to_string(),
      },
      state: State {
        events: Vec::new(),
        until_next_event: INFINITY,
        buffer: RingBuffer::new(16),  // ROB 容量为 16
        pending_dispatch: None,
      },
    }
  }
}

impl DevsModel for Rob {
  fn events_ext( &mut self, msg_input: &ModelMessage, _services: &mut Services,) -> Result<(), SimulationError> {
    // -----------------------------------------------------
    // EnterRoB
    // -----------------------------------------------------
    if msg_input.port_name == self.ports_in.decoded_rob {
      let decoded_inst: DecodedInstruction = serde_json::from_str(&msg_input.content)?;
      log_forward!("ROB: funct={}, domain={}", decoded_inst.funct, decoded_inst.domain_id);
      self.state.events.push(RobEvent::EnterRob(decoded_inst.clone()));
      if self.state.until_next_event == INFINITY {
        self.state.until_next_event = 1.0;
      }
    } else if msg_input.port_name == self.ports_in.rs_ack {
      // 接收 RS 的 ACK/NACK
      let ack: AckMessage = serde_json::from_str(&msg_input.content)?;
      
      if ack.accepted {
        // RS 接受了，清除 pending
        log_backward!("ROB: RS ACK received");
        self.state.pending_dispatch = None;
      } else {
        // RS 拒绝了（busy），pending 保持不变，准备重试
        if let Some((inst, retry_count)) = &mut self.state.pending_dispatch {
          *retry_count += 1;
          log_backward!("ROB: RS NACK ({}), will retry (attempt {})", ack.reason, *retry_count);
        }
        self.state.until_next_event = 2.0;  // 2 cycle 后重试
      }
    }
    Ok(())
  }

  fn events_int(
    &mut self,
    _services: &mut Services,
  ) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut msg_output = Vec::new();
    // -----------------------------------------------------
    // Do retry dispatch to RS
    // -----------------------------------------------------
    if let Some((inst, retry_count)) = &self.state.pending_dispatch {
      msg_output.push(ModelMessage {
        port_name: self.ports_out.to_rs.clone(),
        content: serde_json::to_string(inst)?,
      });
      log_backward!("ROB: retry dispatch funct={} (attempt {})", inst.funct, retry_count);
      self.state.until_next_event = INFINITY;  // 等待 ACK
      return Ok(msg_output);
    }
    // -----------------------------------------------------
    // Handle new events
    // -----------------------------------------------------
    for event in self.state.events.drain(..) {
      match event {
        RobEvent::EnterRob(decoded_inst) => {
          if !self.state.buffer.push_in_rob(decoded_inst.clone()) {
            log_backward!("ROB: buffer full, dropped instruction");
            continue;
          }
          if DispatchPolicy::can_dispatch(&decoded_inst) {
            // 保存到 pending，等待 ACK，初始 retry_count = 0
            self.state.pending_dispatch = Some((decoded_inst.clone(), 0));
            msg_output.push(ModelMessage {
              port_name: self.ports_out.to_rs.clone(),
              content: serde_json::to_string(&decoded_inst)?,
            });
            log_backward!("ROB: dispatch funct={} to RS", decoded_inst.funct);
          }
        }
        RobEvent::CmdCommit(cmd_id) => {
          log_backward!("ROB: CmdCommit cmd_id={}", cmd_id);
        }
      }
    }
    
    if !self.state.buffer.is_empty() {
      if let Some(next_inst) = self.state.buffer.peek() {
        self.state.until_next_event = 1.0;
      }
    } else {
      self.state.until_next_event = INFINITY;
    }
    
    Ok(msg_output)
  }

  fn time_advance(&mut self, time_delta: f64) {self.state.until_next_event -= time_delta;}
  fn until_next_event(&self) -> f64 { self.state.until_next_event }
}

impl Reportable for Rob {
  fn status(&self) -> String {
    format!("ROB - Events: {}, Buffer: {}", self.state.events.len(), self.state.buffer.len())
  }
  fn records(&self) -> &Vec<ModelRecord> {
    static EMPTY: Vec<ModelRecord> = Vec::new();
    &EMPTY
  }
}
impl ReportableModel for Rob {}
impl SerializableModel for Rob {
  fn get_type(&self) -> &'static str {"ROB"}
}
