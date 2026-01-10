use bebop::simulator::sim::mode::{SimConfig, StepMode};
use bebop::simulator::Simulator;
use clap::Parser;

/// Bebop - A RISC-V NPU simulator
#[derive(Parser, Debug)]
#[command(name = "bebop")]
#[command(version = "0.1.0")]
#[command(about = "Bebop simulator developed by buckyball", long_about = None)]
struct Args {
  /// Enable step mode (interactive stepping)
  #[arg(short, long)]
  step: bool,

  /// Quiet mode (suppress log messages)
  #[arg(short, long)]
  quiet: bool,

  /// Output trace file path
  #[arg(long, value_name = "FILE")]
  trace_file: Option<String>,
}

fn main() -> std::io::Result<()> {
  let args = Args::parse();

  let step_mode = if args.step {
    StepMode::Step
  } else {
    StepMode::Continuous
  };

  let config = SimConfig {
    quiet: args.quiet,
    step_mode,
    trace_file: args.trace_file,
  };

  let mut simulator = Simulator::new(config)?;
  simulator.run()
}
