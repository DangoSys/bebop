use crate::buckyball::lib::msg::inject_latency;
use sim::simulator::Simulation;

pub struct Rs;

impl Rs {
  pub fn new() -> Self {
    Self
  }
}

pub fn rs_dispatch(cycle_sim: &mut Simulation, rob_entry: (u32, u32, u64, u64, u8)) -> (u32, u32, u64, u64, u8) {
  let (rob_id, funct, xs1, xs2, domain_id) = rob_entry;

  inject_latency(cycle_sim, "rs", 0.5, None, None, None);

  (rob_id, funct, xs1, xs2, domain_id)
}