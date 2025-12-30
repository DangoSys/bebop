use super::frontend;
use super::simulator::Simulator;

pub struct Npu {
  simulator: Simulator,
}

impl Npu {
  pub fn new() -> Self {
    Self {
      simulator: Simulator::new(),
    }
  }

  pub fn execute(&mut self, inst: usize) {
    let decoded_inst = frontend::decode(inst);
    let msg = frontend::rob_push(decoded_inst);
    self.simulator.send_message(msg);
    self.simulator.step();
  }
}

