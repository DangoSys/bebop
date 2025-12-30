use serde::{Deserialize, Serialize};

/// Decoder 解码后发送给 ROB 的指令
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DispatchedInstruction {
  pub funct: u32,
  pub xs1: u64,
  pub xs2: u64,
  pub domain_id: u8,
  pub rob_id: u64,
}

impl DispatchedInstruction {
  pub fn new(funct: u32, xs1: u64, xs2: u64, domain_id: u8, rob_id: u64) -> Self {
    Self {
      funct,
      xs1,
      xs2,
      domain_id,
      rob_id,
    }
  }
}
