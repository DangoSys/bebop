use std::io;

use sim::simulator::Simulation;
use sim::models::Model;

use crate::buckyball::cycleModel::CycleModel;
use crate::buckyball::frontend::decode_instruction;
use crate::buckyball::lib::msg::inject_latency;

pub struct Buckyball {
  cycle_sim: Simulation,
}

impl Buckyball {
  pub fn new() -> Self {
    let cycle_model = CycleModel::new();
    let models = vec![Model::new("buckyball".to_string(), Box::new(cycle_model))];
    let connectors = vec![];
    let cycle_sim = Simulation::post(models, connectors);

    Self {
      cycle_sim: cycle_sim,
    }
  }

  pub fn inst_execute(&mut self, funct: u32, xs1: u64, xs2: u64) {
    println!("Executing instruction: funct={}, xs1={:#x}, xs2={:#x}", funct, xs1, xs2);
    let decoded_inst1 = decode_instruction(&mut self.cycle_sim, funct, xs1, xs2);
    println!("Decoded instruction 1: {:?}", decoded_inst1);
    let decoded_inst2 = decode_instruction(&mut self.cycle_sim, funct, xs1, xs2);
    println!("Decoded instruction 2: {:?}", decoded_inst2);
  }

  pub fn cycle_advance(&mut self) -> io::Result<Vec<String>> {
    let time1 = self.cycle_sim.get_global_time();
    let messages = self.cycle_sim.step()
      .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{:?}", e)))?;
    let time2 = self.cycle_sim.get_global_time();
    println!("Time: {:.1} -> {:.1}", time1, time2);

    let responses: Vec<String> = messages.iter()
      .filter(|msg| msg.source_id() == "buckyball" && msg.source_port() == "output")
      .map(|msg| msg.content().to_string())
      .collect();

    Ok(responses)
  }
}
