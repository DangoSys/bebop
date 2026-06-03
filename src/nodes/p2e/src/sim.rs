use snafu::{FromString, ResultExt, Whatever};
use std::path::PathBuf;

const FPGA_LOCATION: &str = "0.A";

/// P2E FPGA Simulator CLI
#[derive(Debug, Clone)]
pub struct P2ECli {
    /// Kernel image to load
    pub image: PathBuf,
    /// Bitstream file path
    pub bitstream: PathBuf,
    /// Output directory (VVAC build output)
    pub output: PathBuf,
    /// Log directory
    pub log: PathBuf,
    /// Use multi-FPGA hw_server connection without a location selector
    pub multi_fpga: bool,
    /// Enable waveform dump
    pub wave: bool,
    /// Start waveform dump from this cycle
    pub wave_start: Option<u64>,
}

pub fn run(cli: P2ECli) -> Result<(), Whatever> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Validate paths
    if !cli.image.exists() {
        return Err(Whatever::without_source(format!(
            "Image file not found: {}",
            cli.image.display()
        )));
    }

    if !cli.bitstream.exists() {
        return Err(Whatever::without_source(format!(
            "Bitstream file not found: {}",
            cli.bitstream.display()
        )));
    }

    if !cli.output.exists() {
        return Err(Whatever::without_source(format!(
            "Output directory not found: {}",
            cli.output.display()
        )));
    }

    let case_home = cli
        .output
        .canonicalize()
        .whatever_context("failed to canonicalize output directory")?;

    let rtcfg_path = case_home.join("vvacDir/runtimeDir/rtcfg");
    if !rtcfg_path.exists() {
        return Err(Whatever::without_source(format!(
            "VVAC runtime config not found: {}",
            rtcfg_path.display()
        )));
    }

    // Create log directory
    std::fs::create_dir_all(&cli.log).whatever_context("failed to create log directory")?;

    let uart_log_path = cli.log.join("uart.log");

    log::info!("P2E Simulation Starting");
    log::info!("  Image: {}", cli.image.display());
    log::info!("  Bitstream: {}", cli.bitstream.display());
    log::info!("  FPGA: {}", FPGA_LOCATION);
    log::info!("  Output: {}", case_home.display());
    log::info!("  UART Log: {}", uart_log_path.display());
    log::info!("  Multi FPGA: {}", cli.multi_fpga);
    log::info!("  Waveform: {}", cli.wave);
    log::info!("  Waveform Start Cycle: {}", cli.wave_start.unwrap_or(0));

    // Run simulation
    let result = crate::runner::run(
        FPGA_LOCATION,
        &case_home,
        &rtcfg_path,
        &cli.image,
        &cli.bitstream,
        &cli.log,
        cli.multi_fpga,
        cli.wave,
        cli.wave_start.unwrap_or(0),
    )
    .whatever_context("simulation failed")?;

    // Save UART log
    std::fs::write(&uart_log_path, &result.uart_log).whatever_context("failed to write UART log")?;

    // Report results
    log::info!("Simulation completed!");
    log::info!("  Exit code: {}", result.exit_code);
    log::info!("  Elapsed: {:?}", result.elapsed);
    log::info!("  Cycles: {}", result.cycles);
    log::info!("  UART log: {}", uart_log_path.display());

    if !result.uart_log.is_empty() {
        println!("\n=== UART Output ===");
        println!("{}", result.uart_log);
    }

    if result.exit_code != 0 {
        return Err(Whatever::without_source(format!(
            "Simulation exited with code: {}",
            result.exit_code
        )));
    }

    // Workaround for VVAC library cleanup issue
    log::info!("Exiting bebop-p2e");
    std::process::exit(0);
}
