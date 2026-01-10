use bebop::simulator::sim::mode::{SimConfig, StepMode};
use bebop::simulator::Simulator;

use std::env;

fn main() -> std::io::Result<()> {
  let args: Vec<String> = env::args().collect();

  let step_mode = if args.iter().any(|arg| arg == "--step" || arg == "-s") {
    StepMode::Step
  } else {
    StepMode::Continuous
  };

  let quiet = args.iter().any(|arg| arg == "--quiet" || arg == "-q");

  let config = SimConfig { quiet, step_mode };

  let mut simulator = Simulator::new(config)?;
  simulator.run()
}
