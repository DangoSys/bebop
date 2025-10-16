/// Memory Domain Decoder - 访存指令译码器
use crate::builtin::{Module, Wire};
use crate::global_decoder::DecoderOutput;

/// MemDecoder 输入（来自全局Decoder的输出）
pub type MemDecoderInput = DecoderOutput;

/// MemDecoder 输出
#[derive(Clone, Default)]
pub struct MemDecoderOutput {
  pub is_read: bool,  // 是否为读操作
  pub is_write: bool, // 是否为写操作
  pub addr: u32,      // 访存地址
  pub data: u32,      // 数据
}

/// Memory Domain Decoder - 访存译码器
pub struct MemDecoder {
  name: String,

  // 输入
  pub input: Wire<MemDecoderInput>,

  // 输出
  pub output: Wire<MemDecoderOutput>,
}

impl MemDecoder {
  pub fn new(name: impl Into<String>) -> Self {
    Self {
      name: name.into(),
      input: Wire::default(),
      output: Wire::default(),
    }
  }
}

impl Module for MemDecoder {
  fn run(&mut self) {
    if !self.input.valid {
      self.output.clear();
      return;
    }

    let input = &self.input.value;

    let mut output = MemDecoderOutput {
      is_read: false,
      is_write: false,
      addr: input.xs1 as u32,
      data: input.xs2 as u32,
    };

    let valid = if input.is_mvin {
      // MVIN - 写入scratchpad
      output.is_write = true;
      println!(
        "  [MemDecoder] MVIN -> WRITE: addr=0x{:x}, data=0x{:x}",
        output.addr, output.data
      );
      true
    } else if input.is_mvout {
      // MVOUT - 从scratchpad读出
      output.is_read = true;
      println!("  [MemDecoder] MVOUT -> READ: addr=0x{:x}", output.addr);
      true
    } else {
      false
    };

    if valid {
      self.output.set(output);
    } else {
      self.output.clear();
    }
  }

  fn reset(&mut self) {
    self.input = Wire::default();
    self.output = Wire::default();
  }

  fn name(&self) -> &str {
    &self.name
  }
}
