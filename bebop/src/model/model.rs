use crate::simulator::sim::mode::SimConfig;
use std::io::Result;
use super::npu::Npu;

pub struct Model {
  config: SimConfig,
  npu: Npu,
}

impl Model {
  pub fn new(config: SimConfig) -> Result<Self> {
    Ok(Self {
      config,
      npu: Npu::new(),
    })
  }

  pub fn run(&mut self) -> Result<()> {
    self.npu.execute(1);
    self.npu.execute(2);
    self.npu.execute(3);
    Ok(())
  }
}
