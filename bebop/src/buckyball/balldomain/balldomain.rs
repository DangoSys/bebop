use serde::{Deserialize, Serialize};
use sim::models::model_trait::SerializableModel;
use sim::models::{DevsModel, ModelMessage, ModelRecord, Reportable, ReportableModel};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use bebop_lib::msg::create_message;

/// Compute模块 - 处理计算任务，与Memory交互
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balldomain {
  // PortsIn 字段
  from_frontend: String,
  mem_response: String,
  // PortsOut 字段
  mem_request: String,
  result: String,
  // State 字段
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
      from_frontend: "frontend_balldomain".to_string(),
      mem_response: "memdomain_balldomain".to_string(),
      mem_request: "balldomain_memdomain".to_string(),
      result: "balldomain_result".to_string(),
      phase: Phase::Idle,
      until_next_event: INFINITY,
      records: Vec::new(),
    }
  }
}

impl DevsModel for Balldomain {
  fn events_ext(&mut self, msg_input: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    let current_time = services.global_time();

    if msg_input.port_name == self.from_frontend {
      // 收到Frontend的任务，请求Memory
      self.phase = Phase::WaitingMemory;
      self.until_next_event = 0.0;

      // 使用Services记录事件
      self.records.push(ModelRecord {
        subject: "Balldomain".to_string(),
        time: current_time,
        action: format!("收到任务: {}", msg_input.content),
      });
    } else if msg_input.port_name == self.mem_response {
      // 收到Memory响应，开始计算
      self.phase = Phase::Computing;
      self.until_next_event = 1.0; // 模拟计算耗时1个时间单位

      // 记录内存响应事件
      self.records.push(ModelRecord {
        subject: "Balldomain".to_string(),
        time: current_time,
        action: format!("收到内存数据: {}", msg_input.content),
      });
    }
    Ok(())
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut msg_output = Vec::new();
    let current_time = services.global_time();

    match self.phase {
      Phase::WaitingMemory => {
        // 发送内存请求
        msg_output.push(create_message(&"READ_DATA".to_string(), &self.mem_request)?);

        // 记录发送内存请求
        self.records.push(ModelRecord {
          subject: "Balldomain".to_string(),
          time: current_time,
          action: "发送内存请求".to_string(),
        });

        self.until_next_event = INFINITY; // 等待Memory响应
      },
      Phase::Computing => {
        // 计算完成，输出结果
        let result = format!("RESULT_{}", (current_time * 100.0) as u64);
        msg_output.push(create_message(&result, &self.result)?);

        // 记录计算完成
        self.records.push(ModelRecord {
          subject: "Balldomain".to_string(),
          time: current_time,
          action: format!("计算完成: {}", result),
        });

        self.phase = Phase::Idle;
        self.until_next_event = INFINITY;
      },
      _ => {},
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

impl Reportable for Balldomain {
  fn status(&self) -> String {
    String::new()
  }

  fn records(&self) -> &Vec<ModelRecord> {
    static EMPTY: Vec<ModelRecord> = Vec::new();
    &EMPTY
  }
}

impl ReportableModel for Balldomain {}

impl SerializableModel for Balldomain {
  fn get_type(&self) -> &'static str {
    "Balldomain"
  }
}
