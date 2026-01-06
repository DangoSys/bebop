use std::io::Result;

pub struct TDMA {
}

impl TDMA {
  pub fn new() -> Self {
    Self {}
  }

  pub fn read(&mut self, addr: u64, size: u32) -> Result<u64> {
    Ok(0)
  }

  pub fn write(&mut self, addr: u64, data: u64, size: u32) -> Result<()> {
    Ok(())
  }
}
