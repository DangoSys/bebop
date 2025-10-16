/// Top Module - 顶层模块，连接全局Decoder和各个Domain
use crate::builtin::Module;
use crate::global_decoder::{Decoder, DecoderInput};
use crate::memdomain::MemDomain;

/// Top - NPU顶层模块
pub struct Top {
  name: String,

  // 全局译码器
  pub global_decoder: Decoder,

  // 访存域
  pub memdomain: MemDomain,
}

impl Top {
  pub fn new(name: impl Into<String>, mem_size: usize) -> Self {
    Self {
      name: name.into(),
      global_decoder: Decoder::new("global_decoder"),
      memdomain: MemDomain::new("memdomain", mem_size),
    }
  }

  /// 发送指令
  pub fn send_instruction(&mut self, funct: u64, xs1: u64, xs2: u64) {
    self.global_decoder.input.set(DecoderInput { funct, xs1, xs2 });
  }

  /// 获取访存结果
  pub fn get_mem_data(&self) -> u32 {
    self.memdomain.get_data()
  }

  /// 初始化内存
  pub fn init_mem(&mut self, addr: usize, data: u32) {
    self.memdomain.write(addr, data);
  }
}

impl Module for Top {
  fn run(&mut self) {
    // 从后向前运行：先运行后级模块（读上周期的输入），再运行前级模块（生成本周期的输出）

    // 1. 先运行MemDomain（读取上一周期global_decoder设置的input）
    self.memdomain.run();

    // 2. 再运行全局Decoder（生成本周期的output）
    self.global_decoder.run();

    // 3. 连线更新：本周期的输出 -> 下周期的输入（更新寄存器）
    self.memdomain.decoder.input = self.global_decoder.output.clone();
  }

  fn reset(&mut self) {
    self.global_decoder.reset();
    self.memdomain.reset();
  }

  fn name(&self) -> &str {
    &self.name
  }
}
