use bebop::simulator::host::HostConfig;
use bebop::simulator::sim::mode::{SimConfig, StepMode};
use bebop::simulator::Simulator;
use clap::Parser;
use std::path::PathBuf;

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

  let host_config = HostConfig::default();

  let config = SimConfig {
    quiet: args.quiet,
    step_mode,
    trace_file: args.trace_file,
  };

  let mut simulator = Simulator::new(config, host_config)?;
  simulator.run()
}

#[test]
#[cfg(feature = "bb-tests")]
fn test_mvin_mvout() {
  let test_binary_name = "ctest_mvin_mvout_bebop_test_singlecore-baremetal";
  let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();

  let test_binary = workspace_root
    .join(format!("bb-tests/build/workloads/src/CTest/bebop/{}", test_binary_name))
    .to_string_lossy()
    .to_string();

  let host_path = workspace_root
    .join("bebop/host/spike/riscv-isa-sim/install/bin/spike")
    .to_string_lossy()
    .to_string();

  let host_config = HostConfig {
    host: host_path,
    arg: vec!["--extension=bebop".to_string(), test_binary.to_string()],
  };

  let sim_config = SimConfig {
    quiet: true,
    step_mode: StepMode::Continuous,
    trace_file: None,
  };

  match Simulator::new(sim_config, host_config) {
    Ok(mut simulator) => {
      simulator.run().expect("Simulator run failed");
    },
    Err(e) => {
      panic!("Failed to create simulator: {}", e);
    },
  }
}
