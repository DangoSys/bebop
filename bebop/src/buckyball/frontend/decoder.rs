use sim::simulator::Simulation;
use crate::buckyball::lib::msg::inject_latency;

pub fn decode_funct(funct: u32) -> u8 {
    match funct {
      31 => 0,      // Fence -> domain 0
      24 | 25 => 1, // Load -> domain 1 (memdomain)
      _ => 2,       // Compute -> domain 2 (balldomain)
    }
  }
  
pub fn decode_instruction(cycle_sim: &mut Simulation, raw_inst: u32, xs1: u64, xs2: u64) -> (u8, u64, u64) {
  let funct = decode_funct(raw_inst);
  inject_latency(cycle_sim, "buckyball", 1.0, None, None, None);
  (funct, xs1, xs2)
}
  