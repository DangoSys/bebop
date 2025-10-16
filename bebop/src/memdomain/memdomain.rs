use super::{Bank, Controller, MemDecoder};
/// Memory Domain - 将Decoder、Controller和Bank连接在一起
use crate::builtin::Module;

/// Memory Domain - 包含decoder、controller和bank，并处理它们之间的连线
pub struct MemDomain {
  name: String,
  pub decoder: MemDecoder,
  pub controller: Controller,
  pub bank: Bank,
}

impl MemDomain {
  pub fn new(name: impl Into<String>, bank_size: usize) -> Self {
    Self {
      name: name.into(),
      decoder: MemDecoder::new("mem_decoder"),
      controller: Controller::new("ctrl"),
      bank: Bank::new("bank", bank_size),
    }
  }

  /// 写入数据到bank（用于初始化）
  pub fn write(&mut self, addr: usize, data: u32) {
    self.bank.write(addr, data);
  }

  /// 获取最后读到的数据
  pub fn get_data(&self) -> u32 {
    self.controller.get_data()
  }
}

impl Module for MemDomain {
  fn run(&mut self) {
    // 从后向前运行

    // 1. 先运行Bank（读取上一周期controller设置的req_in）
    self.bank.run();

    // 2. 再运行Controller（读取上一周期bank设置的resp_in）
    self.controller.run();

    // 3. 再运行Decoder（读取上一周期设置的input）
    self.decoder.run();

    // 4. 根据译码结果执行操作（在本周期内的组合逻辑）
    if self.decoder.output.valid {
      let memop = &self.decoder.output.value;

      if memop.is_write {
        // 写操作：直接写入bank
        self.bank.write(memop.addr as usize, memop.data);
      } else if memop.is_read {
        // 读操作：设置Controller的pending请求（下周期生效）
        self.controller.read(memop.addr);
      }
    }

    // 5. 连线更新：本周期的输出 -> 下周期的输入
    self.bank.req_in = self.controller.req_out.clone();
    self.controller.resp_in = self.bank.resp_out.clone();
  }

  fn reset(&mut self) {
    self.decoder.reset();
    self.controller.reset();
    self.bank.reset();
  }

  fn name(&self) -> &str {
    &self.name
  }
}
