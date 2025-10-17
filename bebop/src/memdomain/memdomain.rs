use super::{decoder::DmaOperation, Bank, Controller, MemDecoder};
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

  /// 写入数据到bank（用于初始化，不经过信号线）
  pub fn init_write(&mut self, addr: usize, data: u32) {
    self.bank.init_write(addr, data);
  }

  /// 获取最后读到的数据
  pub fn get_data(&self) -> u32 {
    self.controller.get_data()
  }

  /// 获取 DMA 操作（如果有）
  pub fn get_dma_operation(&self) -> Option<DmaOperation> {
    if self.decoder.output.valid {
      self.decoder.output.value.dma_op.clone()
    } else {
      None
    }
  }

  /// 写入 scratchpad
  pub fn write_spad(&mut self, addr: usize, data: u32) {
    self.bank.init_write(addr, data);
  }

  /// 读取 scratchpad
  pub fn read_spad(&self, addr: usize) -> u32 {
    self.bank.read_data(addr)
  }
}

impl Module for MemDomain {
  fn run(&mut self) {
    // 从后向前运行

    // 1. 先运行Bank（读取上一周期的请求）
    self.bank.run();

    // 2. 再运行Controller（读取上一周期bank的响应）
    self.controller.run();

    // 3. 再运行Decoder（读取上一周期的input）
    self.decoder.run();

    // 4. 连线更新：本周期的输出 -> 下周期的输入
    // 写请求：Decoder -> Bank
    self.bank.write_req = self.decoder.output.value.write_req.clone();

    // 读请求：Decoder -> Controller -> Bank
    self.controller.read_req = self.decoder.output.value.read_req.clone();
    self.bank.read_req = self.controller.req_out.clone();

    // Bank -> Controller 读响应
    self.controller.resp_in = self.bank.read_resp.clone();

    // 传递Decoder输入给Controller（用于DMA操作）
    // 这里需要从Top模块传递原始的DecoderInput
    // 暂时用decoder的input
    // TODO: 需要从Top传递原始的funct, xs1, xs2
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
