// NPU Simulator core: integrates Ball Domain and Mem Domain via BBus

use crate::balldomain::bbus::{BBus, BusTransaction};
use crate::balldomain::instruction::ComputeInstruction;
use crate::balldomain::{BallDomain, BallDomainDecoder};
use crate::global_decoder::{GlobalDecoder, InstructionType};
use crate::memdomain::instruction::MemInstruction;
use crate::memdomain::{MemDomain, MemDomainDecoder};

pub struct NpuSimulator {
  mem_domain: MemDomain,
  ball_domain: BallDomain,
  bbus: BBus,
  global_decoder: GlobalDecoder,
  mem_decoder: MemDomainDecoder,
  ball_decoder: BallDomainDecoder,
}

impl NpuSimulator {
  pub fn new() -> Self {
    Self {
      mem_domain: MemDomain::new(),
      ball_domain: BallDomain::new(),
      bbus: BBus::new(),
      global_decoder: GlobalDecoder::new(),
      mem_decoder: MemDomainDecoder::new(),
      ball_decoder: BallDomainDecoder::new(),
    }
  }

  pub fn alloc_dram(&mut self, addr: u64, size: usize) {
    self.mem_domain.alloc_dram(addr, size);
  }

  pub fn alloc_mem_spad(&mut self, addr: u64, size: usize) {
    self.mem_domain.alloc_spad(addr, size);
  }

  pub fn alloc_ball_spad(&mut self, addr: u64, size: usize) {
    self.ball_domain.alloc_spad(addr, size);
  }

  pub fn write_dram(&mut self, addr: u64, data: Vec<f32>) -> Result<(), String> {
    self.mem_domain.write_dram(addr, data)
  }

  pub fn read_dram(&self, addr: u64, size: usize) -> Result<Vec<f32>, String> {
    self.mem_domain.read_dram(addr, size)
  }

  pub fn execute(&mut self, inst_str: &str) -> Result<(), String> {
    // Step 1: Global Decoder determines instruction type
    let inst_type = self.global_decoder.decode(inst_str)?;

    match inst_type {
      InstructionType::Mem => {
        // Step 2: Mem Domain Decoder decodes memory instruction
        let mem_inst = self.mem_decoder.decode(inst_str)?;
        self.execute_mem_inst(&mem_inst)
      }
      InstructionType::Compute => {
        // Step 2: Ball Domain Decoder decodes compute instruction
        let compute_inst = self.ball_decoder.decode(inst_str)?;
        self.execute_compute_inst(&compute_inst)
      }
    }
  }

  fn execute_mem_inst(&mut self, mem_inst: &MemInstruction) -> Result<(), String> {
    match mem_inst {
      MemInstruction::Mvin { src_addr, dst_addr, size } => {
        // DRAM -> Mem SPAD
        let data = self.mem_domain.read_dram(*src_addr, *size)?;
        self.mem_domain.alloc_spad(*dst_addr, *size);

        println!("[Mem] mvin: DRAM[0x{:x}] -> SPAD[0x{:x}], size={}", src_addr, dst_addr, size);

        // Send to Ball Domain via BBus
        self.bbus.send(BusTransaction {
          src: "MemDomain".to_string(),
          dst: "BallDomain".to_string(),
          addr: *dst_addr,
          data: data.clone(),
        });

        self.ball_domain.write_spad(*dst_addr, data)?;
        Ok(())
      }
      MemInstruction::Mvout { src_addr, dst_addr, size } => {
        // Ball SPAD -> DRAM (read from Ball Domain)
        let data = self.ball_domain.read_spad(*src_addr, *size)?;
        self.mem_domain.write_dram(*dst_addr, data)?;

        println!("[Mem] mvout: Ball-SPAD[0x{:x}] -> DRAM[0x{:x}], size={}", src_addr, dst_addr, size);
        Ok(())
      }
    }
  }

  fn execute_compute_inst(&mut self, compute_inst: &ComputeInstruction) -> Result<(), String> {
    self.ball_domain.execute(compute_inst, &mut self.bbus)?;

    // Clear BBus (computation results stay in Ball Domain)
    while self.bbus.has_pending() {
      self.bbus.recv();
    }

    Ok(())
  }

  pub fn get_cycles(&self) -> u64 {
    self.ball_domain.get_cycles()
  }

  pub fn reset_cycles(&mut self) {
    self.ball_domain.reset_cycles();
  }

  pub fn get_bus_stats(&self) -> usize {
    self.bbus.get_total_transfers()
  }
}
