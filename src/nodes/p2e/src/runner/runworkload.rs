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

/// Run P2E simulation - main entry point
///
/// This is the main entry point for P2E simulation, similar to Verilator's run_batch().
pub fn run(
    fpga_location: &str,
    case_home: &Path,
    rtcfg_path: &Path,
    image: &Path,
    bitstream: &Path,
) -> Result<SimulationResult, String> {
    log::info!("P2E Simulation Starting");
    log::info!("  FPGA: {}", fpga_location);
    log::info!("  Case Home: {}", case_home.display());
    log::info!("  Image: {}", image.display());
    log::info!("  Bitstream: {}", bitstream.display());

    // Validate paths
    if !case_home.exists() {
        return Err(format!("case_home not found: {}", case_home.display()));
    }
    if !rtcfg_path.exists() {
        return Err(format!("rtcfg_path not found: {}", rtcfg_path.display()));
    }
    if !image.exists() {
        return Err(format!("image not found: {}", image.display()));
    }
    if !bitstream.exists() {
        return Err(format!("bitstream not found: {}", bitstream.display()));
    }

    // Source sourceme.sh to set up VVAC environment
    source_environment()?;

    // Configure VVAC environment
    configure_vvac_environment();
    ffi::reset_runtime_state();

    // IMPORTANT: Change to case_home directory before running vdbg
    log::info!("Changing to case_home directory: {}", case_home.display());
    std::env::set_current_dir(case_home)
        .map_err(|e| format!("Failed to change to case_home directory: {}", e))?;
    log::info!("Current directory: {:?}", std::env::current_dir());

    // Generate main.tcl dynamically
    log::info!("Generating main.tcl...");
    let main_tcl = generate_main_tcl(fpga_location, image, bitstream)?;
    let main_tcl_path = case_home.join("main.tcl");
    std::fs::write(&main_tcl_path, main_tcl)
        .map_err(|e| format!("Failed to write main.tcl: {}", e))?;

    // Clean up old flag files before starting
    let flash_done_flag = case_home.join("flash_done.flag");
    let host_init_flag = case_home.join("host_init_done.flag");
    let _ = std::fs::remove_file(&flash_done_flag);
    let _ = std::fs::remove_file(&host_init_flag);
    log::info!("Cleaned up old flag files");

    // Start vdbg with main.tcl in background
    log::info!("Starting vdbg with main.tcl in background...");
    start_vdbg_background(&main_tcl_path)?;

    // Wait for flash to complete
    log::info!("Waiting for bitstream flash to complete...");

    while !flash_done_flag.exists() {
        std::thread::sleep(Duration::from_millis(100));
    }
    log::info!("Flash complete, initializing CTB...");

    // Initialize CTB (connects to running vdbg).
    // IMPORTANT: keep `_ctb` alive until wait_for_completion returns;
    // dropping it triggers quit() and tears down the host-side CTB connection
    // before the workload can run.
    let _ctb = init_ctb(case_home, rtcfg_path)?;

    // Signal TCL that host init is done
    std::fs::write(&host_init_flag, "").map_err(|e| format!("Failed to write host_init_done.flag: {}", e))?;
    log::info!("CTB initialized, signaling TCL to continue...");

    // Wait for simulation to complete
    log::info!("Waiting for simulation to complete...");
    let result = wait_for_completion()?;

    log::info!("Simulation completed");
    log::info!("  Exit code: {}", result.exit_code);
    log::info!("  Elapsed: {:?}", result.elapsed);
    log::info!("  Cycles: {}", result.cycles);

    // _ctb dropped here -> quit() called after the workload finishes.
    Ok(result)
}

/// Initialize CTB (connects to running vdbg)
fn init_ctb(case_home: &Path, rtcfg_path: &Path) -> Result<CtbManager, String> {
    // Create and initialize CTB
    log::info!("Creating CTB manager...");
    let ctb = CtbManager::new()?;

    log::info!("Initializing CTB...");

    // IMPORTANT: The first parameter should be the FPGA config name from rtcfg (e.g., "P0"),
    // NOT the physical FPGA location (e.g., "0.A")
    let fpga_config = "P0";  // Read from rtcfg file: "P0: vc_default"

    // IMPORTANT: case_home must have a trailing slash
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


/// Wait for simulation to complete
fn wait_for_completion() -> Result<SimulationResult, String> {
    let started = Instant::now();
    let poll_interval = Duration::from_millis(100);

    loop {
        // Check if simulation should exit
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

        // Sleep to avoid busy-waiting
        std::thread::sleep(poll_interval);
    }
}

/// Generate main.tcl dynamically based on simulation parameters
fn generate_main_tcl(fpga_location: &str, image: &Path, bitstream: &Path) -> Result<String, String> {
    let script_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/runner");

    let tcl = format!(
        r#"# Main TCL script for P2E simulation
# This script orchestrates the entire P2E simulation flow

set fpga_location "{fpga_location}"
set image "{image}"
set bitstream "{bitstream}"

puts "=========================================="
puts "P2E Simulation Starting"
puts "  FPGA Location: $fpga_location"
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
flash_bitstream $fpga_location

# Step 2: Initialize FPGA and DDR
puts "\n========== Step 2: Initializing FPGA =========="
init_fpga $fpga_location

# Step 3: Load image to DDR
puts "\n========== Step 3: Loading Image =========="
load_image $fpga_location 0 $image

# Step 4: Run workload
puts "\n========== Step 4: Running Workload =========="
run_workload 20000

puts "\n=========================================="
puts "P2E Simulation Completed"
puts "=========================================="

exit
"#,
        fpga_location = fpga_location,
        image = image.display(),
        bitstream = bitstream.display(),
        script_dir = script_dir.display(),
    );

    Ok(tcl)
}

/// Run vdbg with the given TCL script
fn run_vdbg_script(tcl_path: &Path) -> Result<(), String> {
    use duct::cmd;

    let sourceme = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh");
    if !sourceme.exists() {
        return Err(format!("sourceme.sh not found: {}", sourceme.display()));
    }

    let command = format!(
        "source {} && vdbg {}",
        sourceme.display(),
        tcl_path.display()
    );

    log::info!("Executing: {}", command);

    // CRITICAL: Clear LD_PRELOAD to avoid glibc version conflicts
    let output = cmd!("bash", "-c", &command)
        .env_remove("LD_PRELOAD")
        .run()
        .map_err(|e| format!("Failed to run vdbg: {}", e))?;

    if !output.status.success() {
        return Err("vdbg script failed".to_string());
    }

    Ok(())
}

/// Start vdbg in background with the given TCL script
fn start_vdbg_background(tcl_path: &Path) -> Result<(), String> {
    use std::process::Command;

    let sourceme = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh");
    if !sourceme.exists() {
        return Err(format!("sourceme.sh not found: {}", sourceme.display()));
    }

    let command = format!(
        "source {} && vdbg {} &",
        sourceme.display(),
        tcl_path.display()
    );

    log::info!("Starting vdbg in background: {}", command);

    // CRITICAL: Clear LD_PRELOAD to avoid glibc version conflicts
    Command::new("bash")
        .arg("-c")
        .arg(&command)
        .env_remove("LD_PRELOAD")
        .spawn()
        .map_err(|e| format!("Failed to start vdbg: {}", e))?;

    Ok(())
}
/// Source sourceme.sh to set up VVAC environment variables
fn source_environment() -> Result<(), String> {
    use duct::cmd;

    let sourceme = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh");
    if !sourceme.exists() {
        return Err(format!("sourceme.sh not found: {}", sourceme.display()));
    }

    log::info!("Sourcing environment from: {}", sourceme.display());

    // Run bash to source the script and print all environment variables
    let output: String = cmd!("bash", "-c", format!("source {} && env", sourceme.display()))
        .read()
        .map_err(|e| format!("Failed to source sourceme.sh: {}", e))?;

    // Parse the output and set environment variables
    for line in output.lines() {
        if let Some((key, value)) = line.split_once('=') {
            std::env::set_var(key, value);
        }
    }

    log::info!("Environment variables loaded from sourceme.sh");

    // Debug: print key environment variables
    log::info!("HPEC_HOME: {:?}", std::env::var("HPEC_HOME"));
    log::info!("VVAC_HOME: {:?}", std::env::var("VVAC_HOME"));
    log::info!("LD_LIBRARY_PATH: {:?}", std::env::var("LD_LIBRARY_PATH"));

    Ok(())
}

/// Configure VVAC environment variables
///
/// Sets up the environment for VVAC CTB to run in onboard mode (FPGA).
/// This matches the reference example's environment setup.
fn configure_vvac_environment() {
    std::env::set_var("VMRI_LOG_LEVEL", "0");
    std::env::set_var("VVAC_LOG_LEVEL", "0");
    std::env::set_var("RBMGR_LOG_LEVEL", "0");
    std::env::set_var("RBMGR_DUMP_DATA", "1");
    std::env::set_var("RTL_DBG_SIZE", "128");
    // Onboard mode
    std::env::set_var("VMRI_WORK_MODE", "3");
    // Onboard mode
    std::env::set_var("VVAC_WORK_MODE", "0");

    log::info!("Running P2E in onboard mode");
}
