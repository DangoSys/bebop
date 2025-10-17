/// Global Decoder Module - 译码器模块
use crate::builtin::{Module, Wire};

/// Global Decoder 输入
#[derive(Clone, Default)]
pub struct DecoderInput {
  pub funct: u64,
  pub xs1: u64,
  pub xs2: u64,
}

/// Global Decoder 输出
#[derive(Clone, Default)]
pub struct DecoderOutput {
  pub funct: u64, // 原始指令码
  pub xs1: u64,
  pub xs2: u64,
}

/// Global Decoder - 全局译码器
pub struct Decoder {
  name: String,

  // 输入
  pub input: Wire<DecoderInput>,

  // 输出
  pub output: Wire<DecoderOutput>,
}

impl Decoder {
  pub fn new(name: impl Into<String>) -> Self {
    Self {
      name: name.into(),
      input: Wire::default(),
      output: Wire::default(),
    }
  }
}

impl Module for Decoder {
  fn run(&mut self) {
    if !self.input.valid {
      self.output.clear();
      return;
    }

    let input = &self.input.value;

    // 译码逻辑：只负责路由，传递原始指令
    let output = DecoderOutput {
      funct: input.funct,
      xs1: input.xs1,
      xs2: input.xs2,
    };

    // 根据 funct7 值路由指令
    // MVIN_FUNC7 = 24 (0x18)
    // MVOUT_FUNC7 = 25 (0x19)
    let valid = match input.funct {
      24 => {
        // MVIN - 路由到访存域
        println!(
          "[Decoder] MVIN 路由到访存域: funct={}, xs1=0x{:x}, xs2=0x{:x}",
          input.funct, input.xs1, input.xs2
        );
        true
      },
      25 => {
        // MVOUT - 路由到访存域
        println!(
          "[Decoder] MVOUT 路由到访存域: funct={}, xs1=0x{:x}, xs2=0x{:x}",
          input.funct, input.xs1, input.xs2
        );
        true
      },
      _ => {
        println!(
          "[Decoder] UNKNOWN: funct={}, xs1=0x{:x}, xs2=0x{:x}",
          input.funct, input.xs1, input.xs2
        );
        false
      },
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
