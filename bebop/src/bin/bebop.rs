/// Bebop - Accelerator Simulator
///
/// Main executable for running the Bebop accelerator simulator.
/// This program listens for custom instruction requests from Host
/// and simulates accelerator behavior.

use bebop::SimMode;
use bebop::simulator::Simulator;
use std::env;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();

    let sim_mode = if args.iter().any(|arg| arg == "--step" || arg == "-s") {
        SimMode::Step
    } else {
        SimMode::Run
    };

    let mut simulator = Simulator::new(sim_mode);
    simulator.run()
}
