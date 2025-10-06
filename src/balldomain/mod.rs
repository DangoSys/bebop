// Ball Domain: compute units and compute instructions

pub mod bbus;
pub mod domain_decoder;
pub mod instruction;
pub mod mmball;

pub use domain_decoder::BallDomainDecoder;

use bbus::BBus;
use instruction::ComputeInstruction;

pub struct BallDomain {
  compute_unit: mmball::ComputeUnit,
  spad: std::collections::HashMap<u64, Vec<f32>>, // Local scratchpad in ball
}

impl BallDomain {
  pub fn new() -> Self {
    Self {
      compute_unit: mmball::ComputeUnit::new(),
      spad: std::collections::HashMap::new(),
    }
  }

  pub fn alloc_spad(&mut self, addr: u64, size: usize) {
    self.spad.insert(addr, vec![0.0; size]);
  }

  pub fn write_spad(&mut self, addr: u64, data: Vec<f32>) -> Result<(), String> {
    if let Some(mem) = self.spad.get_mut(&addr) {
      if data.len() > mem.len() {
        return Err(format!("Data size {} exceeds allocated size {}", data.len(), mem.len()));
      }
      mem[..data.len()].copy_from_slice(&data);
      Ok(())
    } else {
      Err(format!("Ball SPAD address 0x{:x} not allocated", addr))
    }
  }

  pub fn read_spad(&self, addr: u64, size: usize) -> Result<Vec<f32>, String> {
    if let Some(mem) = self.spad.get(&addr) {
      if size > mem.len() {
        return Err(format!("Read size {} exceeds allocated size {}", size, mem.len()));
      }
      Ok(mem[..size].to_vec())
    } else {
      Err(format!("Ball SPAD address 0x{:x} not allocated", addr))
    }
  }

  pub fn execute(&mut self, inst: &ComputeInstruction, _bbus: &mut BBus) -> Result<(), String> {
    match inst {
      ComputeInstruction::Matmul { a_addr, b_addr, c_addr, m, n, k } => {
        // Use Ball Decoder to decode ball-level operation
        let ball_decoder = mmball::BallDecoder::new();
        let op = ball_decoder.decode_matmul(*a_addr, *b_addr, *c_addr, *m, *n, *k)?;
        
        let a = self.read_spad(op.a_addr, op.m * op.k)?;
        let b = self.read_spad(op.b_addr, op.k * op.n)?;
        let mut c = vec![0.0; op.m * op.n];
        
        self.compute_unit.matmul(&a, &b, &mut c, op.m, op.n, op.k)?;
        self.write_spad(op.c_addr, c)?;
        
        println!(
          "[Ball] matmul: A[0x{:x}]({}×{}) * B[0x{:x}]({}×{}) -> C[0x{:x}]({}×{})",
          op.a_addr, op.m, op.k, op.b_addr, op.k, op.n, op.c_addr, op.m, op.n
        );
        
        Ok(())
      }
    }
  }

  pub fn get_cycles(&self) -> u64 {
    self.compute_unit.get_cycles()
  }

  pub fn reset_cycles(&mut self) {
    self.compute_unit.reset_cycles();
  }
}
