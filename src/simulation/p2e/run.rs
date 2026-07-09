use snafu::Whatever;
#[cfg(feature = "p2e")]
use std::path::PathBuf;

#[cfg(feature = "p2e")]
use bebop_p2e::{self};
#[cfg(feature = "p2e")]
use bebop_uart::{ConsoleConfig, ConsoleServer};
#[cfg(feature = "p2e")]
use snafu::ResultExt;

#[cfg(feature = "p2e")]
const FPGA_LOCATION: &str = "0.A";

#[cfg(feature = "p2e")]
pub struct P2eRunConfig {
    pub image: PathBuf,
    pub bitstream: PathBuf,
    pub log_dir: PathBuf,
    pub multi_fpga: bool,
    pub wave: bool,
    pub wave_start: Option<u64>,
}

#[cfg(feature = "p2e")]
pub fn run(config: P2eRunConfig) -> Result<(), Whatever> {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).try_init();

    if !config.image.exists() {
        snafu::whatever!("P2E image not found: {}", config.image.display());
    }
    if !config.bitstream.exists() {
        snafu::whatever!("bitstream not found: {}", config.bitstream.display());
    }
    if !config.log_dir.exists() {
        snafu::whatever!("log directory not found: {}", config.log_dir.display());
    }

    let case_home = config
        .log_dir
        .canonicalize()
        .whatever_context("failed to canonicalize P2E output directory")?;
    let rtcfg_path = case_home.join("vvacDir/runtimeDir/rtcfg");
    if !rtcfg_path.exists() {
        snafu::whatever!("P2E runtime config not found: {}", rtcfg_path.display());
    }

    let uart_log_path = config.log_dir.join("uart.log");

    log::info!("P2E Simulation Starting");
    log::info!("  Image: {}", config.image.display());
    log::info!("  Bitstream: {}", config.bitstream.display());
    log::info!("  FPGA: {}", FPGA_LOCATION);
    log::info!("  Output: {}", case_home.display());
    log::info!("  UART Log: {}", uart_log_path.display());
    log::info!("  Multi FPGA: {}", config.multi_fpga);
    log::info!("  Waveform: {}", config.wave);
    log::info!("  Waveform Start Cycle: {}", config.wave_start.unwrap_or(0));

    bebop_p2e::source_environment().whatever_context("failed to initialize P2E environment")?;
    bebop_p2e::configure_vvac_environment();
    bebop_p2e::ffi::reset_runtime_state();
    bebop_p2e::ffi::set_log_dir(config.log_dir.to_string_lossy().to_string());

    let console = ConsoleServer::start(&config.log_dir, ConsoleConfig::new("p2e"), bebop_p2e::ffi::push_uart_rx)
        .whatever_context("failed to start P2E console")?;
    bebop_p2e::ffi::set_console_tx(console.tx_sender());

    std::env::set_current_dir(&case_home).whatever_context("failed to enter P2E case directory")?;

    let main_tcl = bebop_p2e::generate_main_tcl(
        FPGA_LOCATION,
        &config.image,
        &config.bitstream,
        config.multi_fpga,
        config.wave,
        config.wave_start.unwrap_or(0),
    )
    .whatever_context("failed to generate P2E main.tcl")?;
    let main_tcl_path = case_home.join("main.tcl");
    std::fs::write(&main_tcl_path, main_tcl).whatever_context("failed to write P2E main.tcl")?;

    let flash_done_flag = case_home.join("flash_done.flag");
    let host_init_flag = case_home.join("host_init_done.flag");
    let sim_exit_flag = case_home.join("sim_exit.flag");
    let _ = std::fs::remove_file(&flash_done_flag);
    let _ = std::fs::remove_file(&host_init_flag);
    let _ = std::fs::remove_file(&sim_exit_flag);

    bebop_p2e::start_vdbg_background(&main_tcl_path).whatever_context("failed to start P2E vdbg")?;
    bebop_p2e::wait_for_flash(&flash_done_flag);

    let _ctb = bebop_p2e::init_ctb(&case_home, &rtcfg_path).whatever_context("failed to initialize P2E CTB")?;
    std::fs::write(&host_init_flag, "").whatever_context("failed to signal P2E host init")?;

    let result = bebop_p2e::wait_for_completion().whatever_context("P2E simulation failed")?;
    drop(console);

    std::fs::write(&uart_log_path, &result.uart_log).whatever_context("failed to write P2E UART log")?;

    log::info!("P2E simulation completed");
    log::info!("  Exit code: {}", result.exit_code);
    log::info!("  Elapsed: {:?}", result.elapsed);
    log::info!("  Cycles: {}", result.cycles);
    log::info!("  UART log: {}", uart_log_path.display());

    if !result.uart_log.is_empty() {
        println!("\n=== UART Output ===");
        println!("{}", result.uart_log);
    }

    if result.exit_code != 0 {
        snafu::whatever!("P2E exited with code {}", result.exit_code);
    }
    Ok(())
}

#[cfg(not(feature = "p2e"))]
pub fn run_unavailable() -> Result<(), Whatever> {
    snafu::whatever!("p2e runner is not compiled into this executable");
}
