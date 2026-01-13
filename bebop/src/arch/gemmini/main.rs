use super::gemmini::Gemmini;
use crate::simulator::server::socket::{DmaReadHandler, DmaWriteHandler};
use std::sync::{Arc, Mutex};

pub struct GemminiSimulation {
  pub gemmini: Gemmini,
}

impl GemminiSimulation {
  pub fn new() -> Self {
    Self {
      gemmini: Gemmini::new(),
    }
  }

  pub fn set_dma_handlers(&mut self, dma_read: Arc<Mutex<DmaReadHandler>>, dma_write: Arc<Mutex<DmaWriteHandler>>) {
    self.gemmini.set_dma_handlers(dma_read, dma_write);
  }

  pub fn execute(&mut self, funct: u64, xs1: u64, xs2: u64) -> u64 {
    self.gemmini.execute(funct, xs1, xs2)
  }

  pub fn reset(&mut self) {
    self.gemmini.reset();
  }
}

pub fn create_gemmini_simulation() -> GemminiSimulation {
  GemminiSimulation::new()
}
