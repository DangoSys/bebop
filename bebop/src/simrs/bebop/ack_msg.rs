use serde::{Deserialize, Serialize};

/// ACK/NACK 消息，用于模块间的握手
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AckMessage {
  pub accepted: bool,   // true = ACK, false = NACK
  pub reason: String,   // 拒绝原因（如 "busy", "full"）
  pub retry_count: u32, // 当前重试次数（仅用于 NACK）
}

impl AckMessage {
  pub fn ack() -> Self {
    Self {
      accepted: true,
      reason: "accepted".to_string(),
      retry_count: 0,
    }
  }

  pub fn nack(reason: &str, retry_count: u32) -> Self {
    Self {
      accepted: false,
      reason: reason.to_string(),
      retry_count,
    }
  }
}
