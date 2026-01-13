use bebop::simulator::host::host::HostConfig;
use bebop::simulator::sim::mode::{ArchType, HostType, SimConfig, StepMode};
use bebop::simulator::utils::log::init_log;
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

  /// Architecture type: buckyball or gemmini (default: buckyball)
  #[arg(short, long, value_name = "ARCH", default_value = "buckyball")]
  arch: String,

  /// Host type: spike or gem5 (default: spike)
  #[arg(long, value_name = "HOST", default_value = "spike")]
  host: String,

  /// Host config file path (default: use default host.toml)
  #[arg(long, value_name = "FILE")]
  host_config: Option<String>,
}

fn main() -> std::io::Result<()> {
  init_log();

  let args = Args::parse();

  let step_mode = if args.step {
    StepMode::Step
  } else {
    StepMode::Continuous
  };

  let arch_type = match args.arch.to_lowercase().as_str() {
    "gemmini" => ArchType::Gemmini,
    "buckyball" => ArchType::Buckyball,
    _ => {
      return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("Unknown architecture: {}", args.arch)));
    }
  };

  let host_type = match args.host.to_lowercase().as_str() {
    "spike" => HostType::Spike,
    "gem5" => HostType::Gem5,
    _ => {
      return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("Unknown host type: {}", args.host)));
    }
  };

  let config = SimConfig {
    quiet: args.quiet,
    step_mode,
    trace_file: args.trace_file,
    arch_type,
    host_type,
    host_config: args.host_config,
  };

  let host_config = HostConfig::from_sim_config(&config);

  let mut simulator = Simulator::new(config, host_config)?;

  simulator.run()
}
