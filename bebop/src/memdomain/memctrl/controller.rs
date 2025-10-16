/// Memory controller
use crate::builtin::{Module, Wire};
use crate::memdomain::mem::bank::{ReadReq, ReadResp};

/// Memory Controller - 向bank发送读请求
pub struct Controller {
  name: String,

  // 输出：读请求
  pub req_out: Wire<ReadReq>,

  // 输入：读响应
  pub resp_in: Wire<ReadResp>,

  // 内部状态
  pending_addr: Option<u32>,
  last_data: u32,
}

impl Controller {
  pub fn new(name: impl Into<String>) -> Self {
    Self {
      name: name.into(),
      req_out: Wire::default(),
      resp_in: Wire::default(),
      pending_addr: None,
      last_data: 0,
    }
  }

  /// 发起读请求
  pub fn read(&mut self, addr: u32) {
    self.pending_addr = Some(addr);
  }

  /// 获取最后读到的数据
  pub fn get_data(&self) -> u32 {
    self.last_data
  }
}

impl Module for Controller {
  fn run(&mut self) {
    // 如果有待发送的读请求
    if let Some(addr) = self.pending_addr {
      self.req_out.set(ReadReq { addr });
      self.pending_addr = None;
    } else {
      self.req_out.clear();
    }

    // 如果收到有效响应
    if self.resp_in.valid {
      self.last_data = self.resp_in.value.data;
    }
  }

  fn reset(&mut self) {
    self.req_out = Wire::default();
    self.resp_in = Wire::default();
    self.pending_addr = None;
    self.last_data = 0;
  }

  fn name(&self) -> &str {
    &self.name
  }
}
