use bebop::model::Model;
use bebop::simulator::sim::mode::{SimConfig, SimMode};

use std::env;

fn main() -> std::io::Result<()> {
  let args: Vec<String> = env::args().collect();

  let sim_mode = if args.iter().any(|arg| arg == "--step" || arg == "-s") {
    SimMode::Step
  } else {
    SimMode::Run
  };

  let quiet = args.iter().any(|arg| arg == "--log" || arg == "-l");

  let config = SimConfig {
    mode: sim_mode,
    quiet: quiet,
  };

  // let mut simulator = Simulator::new(config)?;
  let mut simulator = Model::new(config)?;
  simulator.run()
}
