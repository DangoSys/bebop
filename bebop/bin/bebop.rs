use bebop::simulator::config::config::load_configs;
use bebop::simulator::utils::log::init_log;
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

  /// Architecture type: buckyball or gemmini
  #[arg(short, long, value_name = "ARCH")]
  arch: Option<String>,

  /// Host type: spike or gem5
  #[arg(long, value_name = "HOST")]
  host: Option<String>,

  /// Test binary path
  #[arg(long, value_name = "FILE")]
  test_binary: Option<String>,

  /// Custom config file path (default: use default.toml)
  #[arg(long, value_name = "FILE")]
  config_file: Option<String>,

  /// gem5 SE mode: binary path
  #[arg(long, value_name = "FILE")]
  se_binary: Option<String>,

  /// gem5 FS mode: kernel path
  #[arg(long, value_name = "FILE")]
  fs_kernel: Option<String>,

  /// gem5 FS mode: disk image path
  #[arg(long, value_name = "FILE")]
  fs_image: Option<String>,

  /// gem5 mode: se or fs
  #[arg(long, value_name = "MODE")]
  gem5_mode: Option<String>,
}

fn main() -> std::io::Result<()> {
  init_log();

  let args = Args::parse();

  // Get bebop folder path (CARGO_MANIFEST_DIR)
  let bebop_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").to_path_buf();

  // Load and merge configuration
  let app_config = load_configs(
    args.config_file.as_deref(),
    &bebop_root,
    args.quiet,
    args.step,
    args.trace_file.as_deref(),
    args.arch.as_deref(),
    args.host.as_deref(),
    args.test_binary.as_deref(),
    args.se_binary.as_deref(),
    args.fs_kernel.as_deref(),
    args.fs_image.as_deref(),
    args.gem5_mode.as_deref(),
  )?;

  let mut simulator = Simulator::from_app_config(&app_config)?;
  simulator.run()
}
