use bebop::simulator::Simulator;
/// Bebop - Accelerator Simulator
///
/// Main executable for running the Bebop accelerator simulator.
/// This program listens for custom instruction requests from Host
/// and simulates accelerator behavior.
use bebop::{SimConfig, SimMode};
use std::env;

fn main() -> std::io::Result<()> {
  let args: Vec<String> = env::args().collect();

  let sim_mode = if args.iter().any(|arg| arg == "--step" || arg == "-s") {
    SimMode::Step
  } else {
    SimMode::Run
  };

  let enable_log = args.iter().any(|arg| arg == "--log" || arg == "-l");

  let config = SimConfig {
    mode: sim_mode,
    enable_log,
  };

  let mut simulator = Simulator::new(config)?;
  simulator.run()
}
