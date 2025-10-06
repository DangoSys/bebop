// Memory management: DRAM and on-chip SPAD

use std::collections::HashMap;

#[derive(Debug)]
pub struct Memory {
  dram: HashMap<u64, Vec<f32>>,
  spad: HashMap<u64, Vec<f32>>, // On-chip scratchpad
}

impl Memory {
  pub fn new() -> Self {
    Self {
      dram: HashMap::new(),
      spad: HashMap::new(),
    }
  }

  pub fn alloc_dram(&mut self, addr: u64, size: usize) {
    self.dram.insert(addr, vec![0.0; size]);
  }

  pub fn alloc_spad(&mut self, addr: u64, size: usize) {
    self.spad.insert(addr, vec![0.0; size]);
  }

  pub fn write_dram(&mut self, addr: u64, data: Vec<f32>) -> Result<(), String> {
    if let Some(mem) = self.dram.get_mut(&addr) {
      if data.len() > mem.len() {
        return Err(format!("Data size {} exceeds allocated size {}", data.len(), mem.len()));
      }
      mem[..data.len()].copy_from_slice(&data);
      Ok(())
    } else {
      Err(format!("DRAM address 0x{:x} not allocated", addr))
    }
  }

  pub fn read_dram(&self, addr: u64, size: usize) -> Result<Vec<f32>, String> {
    if let Some(mem) = self.dram.get(&addr) {
      if size > mem.len() {
        return Err(format!("Read size {} exceeds allocated size {}", size, mem.len()));
      }
      Ok(mem[..size].to_vec())
    } else {
      Err(format!("DRAM address 0x{:x} not allocated", addr))
    }
  }

  pub fn write_spad(&mut self, addr: u64, data: Vec<f32>) -> Result<(), String> {
    if let Some(mem) = self.spad.get_mut(&addr) {
      if data.len() > mem.len() {
        return Err(format!("Data size {} exceeds allocated size {}", data.len(), mem.len()));
      }
      mem[..data.len()].copy_from_slice(&data);
      Ok(())
    } else {
      Err(format!("SPAD address 0x{:x} not allocated", addr))
    }
  }

  pub fn read_spad(&self, addr: u64, size: usize) -> Result<Vec<f32>, String> {
    if let Some(mem) = self.spad.get(&addr) {
      if size > mem.len() {
        return Err(format!("Read size {} exceeds allocated size {}", size, mem.len()));
      }
      Ok(mem[..size].to_vec())
    } else {
      Err(format!("SPAD address 0x{:x} not allocated", addr))
    }
  }
}
