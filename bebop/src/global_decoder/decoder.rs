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
  pub is_mvin: bool,
  pub is_mvout: bool,
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

    // 译码逻辑
    let mut output = DecoderOutput {
      is_mvin: false,
      is_mvout: false,
      xs1: input.xs1,
      xs2: input.xs2,
    };

    let valid = match input.funct {
      0 => {
        // MVIN - Move In
        output.is_mvin = true;
        println!("[Decoder] MVIN: xs1=0x{:x}, xs2=0x{:x}", input.xs1, input.xs2);
        true
      },
      1 => {
        // MVOUT - Move Out
        output.is_mvout = true;
        println!("[Decoder] MVOUT: xs1=0x{:x}, xs2=0x{:x}", input.xs1, input.xs2);
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
