use bebop::simulator::sim::mode::{RunMode, SimConfig, StepMode};
use bebop::simulator::Simulator;

use std::env;

fn main() -> std::io::Result<()> {
  let args: Vec<String> = env::args().collect();

  let step_mode = if args.iter().any(|arg| arg == "--step" || arg == "-s") {
    StepMode::Step
  } else {
    StepMode::Continuous
  };
  let quiet = args.iter().any(|arg| arg == "--log" || arg == "-l");
  let run_mode = if args.iter().any(|arg| arg == "--cycle" || arg == "-c") {
    RunMode::Cycle
  } else {
    RunMode::Func
  };
  let config = SimConfig {
    run_mode,
    quiet,
    step_mode,
  };

  let mut simulator = Simulator::new(config)?;
  simulator.run()
}
