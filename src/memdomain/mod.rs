// Mem Domain: on-chip SPAD and memory instructions

pub mod domain_decoder;
pub mod instruction;
pub mod memory;

pub use domain_decoder::MemDomainDecoder;

pub struct MemDomain {
  memory: memory::Memory,
}

impl MemDomain {
  pub fn new() -> Self {
    Self {
      memory: memory::Memory::new(),
    }
  }

  pub fn alloc_dram(&mut self, addr: u64, size: usize) {
    self.memory.alloc_dram(addr, size);
  }

  pub fn alloc_spad(&mut self, addr: u64, size: usize) {
    self.memory.alloc_spad(addr, size);
  }

  pub fn write_dram(&mut self, addr: u64, data: Vec<f32>) -> Result<(), String> {
    self.memory.write_dram(addr, data)
  }

  pub fn read_dram(&self, addr: u64, size: usize) -> Result<Vec<f32>, String> {
    self.memory.read_dram(addr, size)
  }
}
