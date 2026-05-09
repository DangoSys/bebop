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
pub fn run(fpga_location: &str, case_home: &Path, rtcfg_path: &Path, image: &Path) -> Result<SimulationResult, String> {
    log::info!("P2E Simulation Starting");
    log::info!("  FPGA: {}", fpga_location);
    log::info!("  Case Home: {}", case_home.display());
    log::info!("  Image: {}", image.display());

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

    // Step 1: Initialize simulation (CTB)
    log::info!("Step 1: Initializing simulation (CTB)...");
    init_sim(fpga_location, case_home, rtcfg_path)?;

    // Step 2: Flash bitstream
    log::info!("Step 2: Flashing bitstream...");
    flash_bitstream(fpga_location, case_home)?;

    // Step 3: Initialize FPGA
    log::info!("Step 3: Initializing FPGA...");
    init_fpga(case_home)?;

    // Step 4: Load image to DDR
    log::info!("Step 4: Loading image to DDR...");
    load_image(fpga_location, case_home, image)?;

    // Step 5: Run workload
    log::info!("Step 5: Running workload...");
    let result = run_workload()?;

    log::info!("Simulation completed");
    log::info!("  Exit code: {}", result.exit_code);
    log::info!("  Elapsed: {:?}", result.elapsed);
    log::info!("  Cycles: {}", result.cycles);

    Ok(result)
}

/// Initialize simulation (CTB)
fn init_sim(fpga_location: &str, case_home: &Path, rtcfg_path: &Path) -> Result<(), String> {
    // Configure VVAC environment
    configure_vvac_environment();
    ffi::reset_runtime_state();

    // Create and initialize CTB
    log::info!("Creating CTB manager...");
    let ctb = CtbManager::new()?;

    log::info!("Initializing CTB...");
    ctb.init(
        fpga_location,
        case_home.to_str().ok_or("Invalid case_home path")?,
        rtcfg_path.to_str().ok_or("Invalid rtcfg_path")?,
    )?;

    ffi::mark_initialized();
    log::info!("CTB initialized successfully");

    Ok(())
}

/// Flash bitstream to FPGA
fn flash_bitstream(fpga_location: &str, case_home: &Path) -> Result<(), String> {
    use crate::runner::flashbitstream::FlashBitstreamStep;

    // Find bitstream file in case_home
    let bitstream_path = case_home.join("design.bit");
    if !bitstream_path.exists() {
        return Err(format!("Bitstream not found: {}", bitstream_path.display()));
    }

    let step = FlashBitstreamStep::new(bitstream_path)
        .fpga_location(fpga_location)
        .output_dir(case_home);

    step.run()
}

/// Initialize FPGA and check DDR calibration
fn init_fpga(case_home: &Path) -> Result<(), String> {
    use crate::runner::init::InitStep;

    let step = InitStep::new(case_home);
    step.run()
}

/// Load image to DDR
fn load_image(fpga_location: &str, case_home: &Path, image: &Path) -> Result<(), String> {
    use std::process::Command;

    log::info!("Loading image to DDR via vdbg...");

    // Find sourceme.sh
    let sourceme = find_sourceme(case_home)?;

    // Generate TCL script to load image
    let tcl_script = format!(
        r#"
# Load workload.tcl
source {}/workload.tcl

# Call load_image function
load_image {} 0 {}

exit
"#,
        case_home.join("../src/runner/2_runworkload").display(),
        fpga_location,
        image.display()
    );

    let tcl_path = case_home.join("load_image.tcl");
    std::fs::write(&tcl_path, tcl_script).map_err(|e| format!("Failed to write load_image.tcl: {}", e))?;

    // Run vdbg to execute the TCL script
    let cmd = format!(
        "source {} && cd {} && vdbg load_image.tcl",
        sourceme.display(),
        case_home.display()
    );

    log::info!("Executing: {}", cmd);

    let status = Command::new("bash")
        .arg("-c")
        .arg(&cmd)
        .status()
        .map_err(|e| format!("Failed to execute vdbg: {}", e))?;

    if !status.success() {
        return Err("vdbg load_image failed".to_string());
    }

    log::info!("Image loaded successfully");
    Ok(())
}

/// Find sourceme.sh
fn find_sourceme(case_home: &Path) -> Result<std::path::PathBuf, String> {
    let candidates = vec![
        std::path::PathBuf::from("sourceme.sh"),
        case_home.join("../sourceme.sh"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh"),
    ];

    for path in candidates {
        if path.exists() {
            return Ok(path);
        }
    }

    Err("sourceme.sh not found".to_string())
}

/// Run workload until exit
fn run_workload() -> Result<SimulationResult, String> {
    let started = Instant::now();
    let mut cycles: u64 = 0;

    // Advance 1000 cycles per step
    let step_cycles = 1000;
    let poll_interval = Duration::from_millis(10); // Poll every 10ms

    loop {
        // Check if simulation should exit
        if ffi::check_exit() {
            let exit_code = ffi::exit_code();
            let uart_log = ffi::uart_log();

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

/// Run simulation until exit signal is asserted (internal helper)
///
/// Similar to Verilator's run_batch(), this runs the simulation loop
/// until the RTL asserts the exit signal via scu_sim_exit() DPI-C call.
#[allow(dead_code)]
fn run_until_exit_internal() -> Result<SimulationResult, String> {
    let started = Instant::now();
    let mut cycles: u64 = 0;

    let step_cycles = 1000; // Advance 1000 cycles per step
    let poll_interval = Duration::from_millis(10); // Poll every 10ms

    loop {
        // Check if simulation should exit
        if ffi::check_exit() {
            let exit_code = ffi::exit_code();
            let uart_log = ffi::uart_log();

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
    std::env::set_var("VMRI_WORK_MODE", "3"); // Onboard mode
    std::env::set_var("VVAC_WORK_MODE", "0"); // Onboard mode
    log::info!("Running P2E in onboard mode");
}
