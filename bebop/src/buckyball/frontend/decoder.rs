use crate::buckyball::lib::msg::inject_latency;
use sim::simulator::Simulation;

pub fn decode_funct(funct: u32) -> u8 {
  match funct {
    31 => 0,      // Fence -> domain 0
    24 | 25 => 1, // Load -> domain 1 (memdomain)
    _ => 2,       // Compute -> domain 2 (balldomain)
  }
}

pub fn global_decode(cycle_sim: &mut Simulation, (funct, xs1, xs2): (Option<u32>, Option<u64>, Option<u64>)) -> (u32, u64, u64, u8) {
  let funct = funct.unwrap();
  let xs1 = xs1.unwrap();
  let xs2 = xs2.unwrap();
  let domain_id = decode_funct(funct);
  inject_latency(cycle_sim, "decoder", 0.5, None, None, None);
  (funct, xs1, xs2, domain_id)
}
