/// Memory Domain Decoder - 访存指令译码器
use crate::builtin::{Module, Wire};
use crate::global_decoder::DecoderOutput;

/// MemDecoder 输入（来自全局Decoder的输出）
pub type MemDecoderInput = DecoderOutput;

use crate::memdomain::mem::bank::{ReadReq, WriteReq};

/// MemDecoder 输出
#[derive(Clone, Default)]
pub struct MemDecoderOutput {
  pub read_req: Wire<ReadReq>,   // 读请求信号线
  pub write_req: Wire<WriteReq>, // 写请求信号线
  pub is_dma: bool,              // 是否为DMA操作
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
    let addr = input.xs1 as u32;
    let data = input.xs2 as u32;

    let mut output = MemDecoderOutput::default();

    match input.funct {
      0 => {
        // MVIN - 写入scratchpad + DMA
        output.write_req.set(WriteReq { addr, data });
        output.read_req.clear();
        output.is_dma = true;
        println!("  [MemDecoder] MVIN -> WRITE: addr=0x{:x}, data=0x{:x}", addr, data);
      },
      1 => {
        // MVOUT - 从scratchpad读出 + DMA
        output.read_req.set(ReadReq { addr });
        output.write_req.clear();
        output.is_dma = true;
        println!("  [MemDecoder] MVOUT -> READ: addr=0x{:x}", addr);
      },
      _ => {
        println!("  [MemDecoder] UNKNOWN: funct={}", input.funct);
        output.read_req.clear();
        output.write_req.clear();
        output.is_dma = false;
      },
    }

    self.output.set(output);
  }

  fn reset(&mut self) {
    self.input = Wire::default();
    self.output = Wire::default();
  }

  fn name(&self) -> &str {
    &self.name
  }
}
