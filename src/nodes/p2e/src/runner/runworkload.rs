use crate::ffi::{self, CtbManager};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub exit_code: i32,
    pub elapsed: Duration,
    pub cycles: u64,
    pub uart_log: String,
}

pub fn init_ctb(case_home: &Path, rtcfg_path: &Path) -> Result<CtbManager, String> {
    log::info!("Creating CTB manager...");
    let ctb = CtbManager::new()?;

    log::info!("Initializing CTB...");

    let fpga_config = "P0"; // Read from rtcfg file: "P0: vc_default"
    let case_home_str = format!("{}/", case_home.display());

    log::info!("  fpga_config: {}", fpga_config);
    log::info!("  case_home: {}", case_home_str);
    log::info!("  rtcfg_path: {}", rtcfg_path.display());

    ctb.init(
        fpga_config,
        &case_home_str,
        rtcfg_path.to_str().ok_or("Invalid rtcfg_path")?,
    )?;

    ffi::mark_initialized();
    log::info!("CTB initialized successfully");

    Ok(ctb)
}

pub fn wait_for_completion() -> Result<SimulationResult, String> {
    let started = Instant::now();
    let poll_interval = Duration::from_millis(100);

    loop {
        if ffi::check_exit() {
            let exit_code = ffi::exit_code();
            let uart_log = ffi::uart_log();
            let cycles = 1000;

            return Ok(SimulationResult {
                exit_code,
                elapsed: started.elapsed(),
                cycles,
                uart_log,
            });
        }

        std::thread::sleep(poll_interval);
    }
}

pub fn generate_main_tcl(
    fpga_location: &str,
    image: &Path,
    bitstream: &Path,
    multi_fpga: bool,
    wave: bool,
    wave_start: u64,
) -> Result<String, String> {
    let script_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runner");

    let tcl = format!(
        r#"# Main TCL script for P2E simulation
# This script orchestrates the entire P2E simulation flow

set fpga_location "{fpga_location}"
set image "{image}"
set bitstream "{bitstream}"
set multi_fpga {multi_fpga}
set wave {wave}
set wave_start {wave_start}

puts "=========================================="
puts "P2E Simulation Starting"
puts "  FPGA Location: $fpga_location"
puts "  Multi FPGA: $multi_fpga"
puts "  Bitstream: $bitstream"
puts "  Image: $image"
puts "=========================================="

# Load all TCL modules
set script_dir "{script_dir}"
source $script_dir/0_flashbitstream/flash.tcl
source $script_dir/1_init/init.tcl
source $script_dir/2_runworkload/workload.tcl

# Step 1: Flash bitstream
puts "\n========== Step 1: Flashing Bitstream =========="
flash_bitstream $fpga_location $multi_fpga

# Step 2: Initialize FPGA and DDR
puts "\n========== Step 2: Initializing FPGA =========="
init_fpga $fpga_location

# Step 3: Load image to DDR
puts "\n========== Step 3: Loading Image =========="
load_image $fpga_location 0 $image

# Step 4: Run workload
puts "\n========== Step 4: Running Workload =========="
run_workload 20000 $wave $wave_start

puts "\n=========================================="
puts "P2E Simulation Completed"
puts "=========================================="

exit
"#,
        fpga_location = fpga_location,
        image = image.display(),
        bitstream = bitstream.display(),
        multi_fpga = if multi_fpga { 1 } else { 0 },
        wave = if wave { 1 } else { 0 },
        wave_start = wave_start,
        script_dir = script_dir.display(),
    );

    Ok(tcl)
}

pub fn start_vdbg_background(tcl_path: &Path) -> Result<(), String> {
    use std::process::Command;

    let sourceme = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh");
    if !sourceme.exists() {
        return Err(format!("sourceme.sh not found: {}", sourceme.display()));
    }

    let command = format!("source {} && vdbg {} &", sourceme.display(), tcl_path.display());

    log::info!("Starting vdbg in background: {}", command);

    Command::new("bash")
        .arg("-c")
        .arg(&command)
        .env_remove("LD_PRELOAD")
        .spawn()
        .map_err(|e| format!("Failed to start vdbg: {}", e))?;

    Ok(())
}

pub fn source_environment() -> Result<(), String> {
    use duct::cmd;

    let sourceme = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh");
    if !sourceme.exists() {
        return Err(format!("sourceme.sh not found: {}", sourceme.display()));
    }

    log::info!("Sourcing environment from: {}", sourceme.display());

    let output: String = cmd!("bash", "-c", format!("source {} && env", sourceme.display()))
        .read()
        .map_err(|e| format!("Failed to source sourceme.sh: {}", e))?;

    for line in output.lines() {
        if let Some((key, value)) = line.split_once('=') {
            std::env::set_var(key, value);
        }
    }

    log::info!("Environment variables loaded from sourceme.sh");
    log::info!("HPEC_HOME: {:?}", std::env::var("HPEC_HOME"));
    log::info!("VVAC_HOME: {:?}", std::env::var("VVAC_HOME"));
    log::info!("LD_LIBRARY_PATH: {:?}", std::env::var("LD_LIBRARY_PATH"));

    Ok(())
}

pub fn configure_vvac_environment() {
    std::env::set_var("VMRI_LOG_LEVEL", "0");
    std::env::set_var("VVAC_LOG_LEVEL", "0");
    std::env::set_var("RBMGR_LOG_LEVEL", "0");
    std::env::set_var("RBMGR_DUMP_DATA", "1");
    std::env::set_var("RTL_DBG_SIZE", "128");
    std::env::set_var("VMRI_WORK_MODE", "3");
    std::env::set_var("VVAC_WORK_MODE", "0");

    log::info!("Running P2E in onboard mode");
}

pub fn wait_for_flash(flash_done_flag: &Path) {
    while !flash_done_flag.exists() {
        std::thread::sleep(Duration::from_millis(100));
    }
}
