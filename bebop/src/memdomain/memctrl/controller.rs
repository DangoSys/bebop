/// Memory controller
use crate::builtin::{Module, Wire};
use crate::memdomain::mem::bank::{ReadReq, ReadResp};

/// Memory Controller - 向bank发送读请求
pub struct Controller {
  name: String,

  // 输入：读请求信号
  pub read_req: Wire<ReadReq>,

  // 输出：读请求
  pub req_out: Wire<ReadReq>,

  // 输入：读响应
  pub resp_in: Wire<ReadResp>,

  // 内部状态
  last_data: u32,
}

impl Controller {
  pub fn new(name: impl Into<String>) -> Self {
    Self {
      name: name.into(),
      read_req: Wire::default(),
      req_out: Wire::default(),
      resp_in: Wire::default(),
      last_data: 0,
    }
  }

  /// 获取最后读到的数据
  pub fn get_data(&self) -> u32 {
    self.last_data
  }
}

impl Module for Controller {
  fn run(&mut self) {
    // 处理读请求信号
    if self.read_req.valid {
      self.req_out.set(self.read_req.value.clone());
    } else {
      self.req_out.clear();
    }

    // 如果收到有效响应
    if self.resp_in.valid {
      self.last_data = self.resp_in.value.data;
    }
  }

  fn reset(&mut self) {
    self.read_req = Wire::default();
    self.req_out = Wire::default();
    self.resp_in = Wire::default();
    self.last_data = 0;
  }

  fn name(&self) -> &str {
    &self.name
  }
}
