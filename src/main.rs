mod balldomain;
mod global_decoder;
mod memdomain;
mod simulator;

use simulator::NpuSimulator;

// Public API: custom instruction interface
pub fn custom_inst(sim: &mut NpuSimulator, inst_str: &str) -> Result<(), String> {
  sim.execute(inst_str)
}

fn main() {
  println!("Bebop NPU Simulator");
  println!("===================");
  println!("Architecture: Ball Domain + Mem Domain + BBus\n");

  let mut sim = NpuSimulator::new();

  // Setup memory in Mem Domain
  sim.alloc_dram(0x1000, 16);
  sim.alloc_dram(0x2000, 16);
  sim.alloc_dram(0x3000, 16);
  sim.alloc_mem_spad(0x100, 16);
  sim.alloc_mem_spad(0x200, 16);
  sim.alloc_mem_spad(0x300, 16);
  
  // Setup Ball Domain SPAD
  sim.alloc_ball_spad(0x100, 16);
  sim.alloc_ball_spad(0x200, 16);
  sim.alloc_ball_spad(0x300, 16);

  // Initialize data: 2×2 matrix A and B in DRAM
  let a = vec![1.0, 2.0, 3.0, 4.0];
  let b = vec![5.0, 6.0, 7.0, 8.0];
  sim.write_dram(0x1000, a).unwrap();
  sim.write_dram(0x2000, b).unwrap();

  println!("Example: 2×2 matrix multiplication");
  println!("A = [1.0, 2.0; 3.0, 4.0]");
  println!("B = [5.0, 6.0; 7.0, 8.0]\n");

  // Execute instructions via custom_inst interface
  println!("Executing instructions:");
  custom_inst(&mut sim, "mvin 0x1000 0x100 4").unwrap();
  custom_inst(&mut sim, "mvin 0x2000 0x200 4").unwrap();
  custom_inst(&mut sim, "matmul 0x100 0x200 0x300 2 2 2").unwrap();
  custom_inst(&mut sim, "mvout 0x300 0x3000 4").unwrap();

  // Read result from DRAM
  let result = sim.read_dram(0x3000, 4).unwrap();
  println!("\n===================");
  println!("Result C = {:?}", result);
  println!("Total compute cycles: {}", sim.get_cycles());
  println!("Total bus transfers: {}", sim.get_bus_stats());
}
