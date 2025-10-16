/// Memory bank with scratchpad
use crate::builtin::{Module, Wire};

/// 读请求
#[derive(Clone, Default)]
pub struct ReadReq {
  pub addr: u32,
}

/// 读响应
#[derive(Clone, Default)]
pub struct ReadResp {
  pub data: u32,
}

/// Memory Bank - 简单的scratchpad存储
pub struct Bank {
  name: String,

  // 输入：读请求
  pub req_in: Wire<ReadReq>,

  // 输出：读响应
  pub resp_out: Wire<ReadResp>,

  // Scratchpad存储
  spad: Vec<u32>,
}

impl Bank {
  pub fn new(name: impl Into<String>, size: usize) -> Self {
    Self {
      name: name.into(),
      req_in: Wire::default(),
      resp_out: Wire::default(),
      spad: vec![0; size],
    }
  }

  /// 写入数据到scratchpad（用于初始化）
  pub fn write(&mut self, addr: usize, data: u32) {
    if addr < self.spad.len() {
      self.spad[addr] = data;
    }
  }
}

impl Module for Bank {
  fn run(&mut self) {
    // 如果有有效的读请求
    if self.req_in.valid {
      let addr = self.req_in.value.addr as usize;

      // 从scratchpad读取数据
      let data = if addr < self.spad.len() {
        self.spad[addr]
      } else {
        0 // 越界返回0
      };

      // 返回响应
      self.resp_out.set(ReadResp { data });
    } else {
      // 没有请求，清空响应
      self.resp_out.clear();
    }
  }

  fn reset(&mut self) {
    self.req_in = Wire::default();
    self.resp_out = Wire::default();
    self.spad.fill(0);
  }

  fn name(&self) -> &str {
    &self.name
  }
}
