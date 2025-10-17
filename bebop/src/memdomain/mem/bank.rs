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

/// 写请求
#[derive(Clone, Default)]
pub struct WriteReq {
  pub addr: u32,
  pub data: u32,
}

/// Memory Bank - 简单的scratchpad存储
pub struct Bank {
  name: String,

  // 输入：读请求
  pub read_req: Wire<ReadReq>,

  // 输入：写请求
  pub write_req: Wire<WriteReq>,

  // 输出：读响应
  pub read_resp: Wire<ReadResp>,

  // Scratchpad存储
  spad: Vec<u32>,
}

impl Bank {
  pub fn new(name: impl Into<String>, size: usize) -> Self {
    Self {
      name: name.into(),
      read_req: Wire::default(),
      write_req: Wire::default(),
      read_resp: Wire::default(),
      spad: vec![0; size],
    }
  }

  /// 直接写入数据（用于初始化，不经过信号线）
  pub fn init_write(&mut self, addr: usize, data: u32) {
    if addr < self.spad.len() {
      self.spad[addr] = data;
    }
  }

  /// 直接读取数据（用于 DMA，不经过信号线）
  pub fn read_data(&self, addr: usize) -> u32 {
    if addr < self.spad.len() {
      self.spad[addr]
    } else {
      0 // 越界返回0
    }
  }
}

impl Module for Bank {
  fn run(&mut self) {
    // 处理写请求
    if self.write_req.valid {
      let addr = self.write_req.value.addr as usize;
      if addr < self.spad.len() {
        self.spad[addr] = self.write_req.value.data;
      }
    }

    // 处理读请求
    if self.read_req.valid {
      let addr = self.read_req.value.addr as usize;
      let data = if addr < self.spad.len() {
        self.spad[addr]
      } else {
        0 // 越界返回0
      };
      self.read_resp.set(ReadResp { data });
    } else {
      self.read_resp.clear();
    }
  }

  fn reset(&mut self) {
    self.read_req = Wire::default();
    self.write_req = Wire::default();
    self.read_resp = Wire::default();
    self.spad.fill(0);
  }

  fn name(&self) -> &str {
    &self.name
  }
}
