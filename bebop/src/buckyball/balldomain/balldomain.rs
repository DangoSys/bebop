use serde::{Deserialize, Serialize};
use sim::simulator::Services;
use sim::models::{ModelRecord, DevsModel, Reportable, ReportableModel, ModelMessage};
use sim::models::model_trait::SerializableModel;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

/// Compute模块 - 处理计算任务，与Memory交互
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balldomain {
  ports_in: PortsIn,
  ports_out: PortsOut,
  state: State,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortsIn {
  from_frontend: String,
  mem_response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortsOut {
  mem_request: String,
  result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
  phase: Phase,
  until_next_event: f64,
  records: Vec<ModelRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum Phase {
  Idle,
  WaitingMemory,
  Computing,
}

impl Balldomain {
  pub fn new() -> Self {
    Self {
      ports_in: PortsIn {
        from_frontend: "frontend_balldomain".to_string(),
        mem_response: "memdomain_balldomain".to_string(),
      },
      ports_out: PortsOut {
        mem_request: "balldomain_memdomain".to_string(),
        result: "balldomain_result".to_string(),
      },
      state: State {
        phase: Phase::Idle,
        until_next_event: INFINITY,
        records: Vec::new(),
      },
    }
  }
}

impl DevsModel for Balldomain {
  fn events_ext(
    &mut self,
    msg_input: &ModelMessage,
    services: &mut Services,
  ) -> Result<(), SimulationError> {
    let current_time = services.global_time();
    
    if msg_input.port_name == self.ports_in.from_frontend {
      // 收到Frontend的任务，请求Memory
      self.state.phase = Phase::WaitingMemory;
      self.state.until_next_event = 0.0;
      
      // 使用Services记录事件
      self.state.records.push(ModelRecord {
        subject: "Balldomain".to_string(),
        time: current_time,
        action: format!("收到任务: {}", msg_input.content),
      });
    } else if msg_input.port_name == self.ports_in.mem_response {
      // 收到Memory响应，开始计算
      self.state.phase = Phase::Computing;
      self.state.until_next_event = 1.0; // 模拟计算耗时1个时间单位
      
      // 记录内存响应事件
      self.state.records.push(ModelRecord {
        subject: "Balldomain".to_string(),
        time: current_time,
        action: format!("收到内存数据: {}", msg_input.content),
      });
    }
    Ok(())
  }

  fn events_int(
    &mut self,
    services: &mut Services,
  ) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut msg_output = Vec::new();
    let current_time = services.global_time();
    
    match self.state.phase {
      Phase::WaitingMemory => {
        // 发送内存请求
        msg_output.push(ModelMessage {
          port_name: self.ports_out.mem_request.clone(),
          content: "READ_DATA".to_string(),
        });
        
        // 记录发送内存请求
        self.state.records.push(ModelRecord {
          subject: "Balldomain".to_string(),
          time: current_time,
          action: "发送内存请求".to_string(),
        });
        
        self.state.until_next_event = INFINITY; // 等待Memory响应
      }
      Phase::Computing => {
        // 计算完成，输出结果
        let result = format!("RESULT_{}", (current_time * 100.0) as u64);
        msg_output.push(ModelMessage {
          port_name: self.ports_out.result.clone(),
          content: result.clone(),
        });
        
        // 记录计算完成
        self.state.records.push(ModelRecord {
          subject: "Balldomain".to_string(),
          time: current_time,
          action: format!("计算完成: {}", result),
        });
        
        self.state.phase = Phase::Idle;
        self.state.until_next_event = INFINITY;
      }
      _ => {}
    }
    
    Ok(msg_output)
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.state.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    self.state.until_next_event
  }
}

impl Reportable for Balldomain {
  fn status(&self) -> String {
    format!("Balldomain - Phase: {:?}", self.state.phase)
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.state.records
  }
}

impl ReportableModel for Balldomain {}

impl SerializableModel for Balldomain {
  fn get_type(&self) -> &'static str {
    "Balldomain"
  }
}
