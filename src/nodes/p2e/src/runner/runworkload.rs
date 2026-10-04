use crate::ffi::{self, CtbManager};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub exit_code: i32,
    pub elapsed: Duration,
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

pub fn wait_for_completion(mut poll: impl FnMut() -> Result<(), String>) -> Result<SimulationResult, String> {
    let started = Instant::now();
    let poll_interval = Duration::from_millis(100);

    loop {
        poll()?;
        if ffi::check_exit() {
            let exit_code = ffi::exit_code();
            let uart_log = ffi::uart_log();

            return Ok(SimulationResult {
                exit_code,
                elapsed: started.elapsed(),
                uart_log,
            });
        }

        std::thread::sleep(poll_interval);
    }
}

pub(super) fn tcl_word(value: &str) -> String {
    let mut quoted = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' | '"' | '$' | '[' | ']' => {
                quoted.push('\\');
                quoted.push(ch);
            }
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

pub fn generate_main_tcl(
    fpga_location: &str,
    plan: &super::LoadPlan,
    bitstream: &Path,
    multi_fpga: bool,
    wave: bool,
    wave_start: u64,
) -> Result<String, String> {
    let script_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runner");
    let path_word =
        |path: &Path| -> Result<String, String> { Ok(tcl_word(path.to_str().ok_or("Tcl paths must be UTF-8")?)) };
    let mut tcl = format!(
        "set fpga_location {}\nset bitstream {}\nset multi_fpga {}\nset wave {}\nset wave_start {}\n",
        tcl_word(fpga_location),
        path_word(bitstream)?,
        u8::from(multi_fpga),
        u8::from(wave),
        wave_start
    );
    for script in [
        "0_flashbitstream/flash.tcl",
        "1_init/init.tcl",
        "2_runworkload/workload.tcl",
    ] {
        tcl.push_str(&format!("source {}\n", path_word(&script_dir.join(script))?));
    }
    tcl.push_str("flash_bitstream $fpga_location $multi_fpga\ninit_fpga $fpga_location\n");
    for load in &plan.loads {
        tcl.push_str(&format!(
            "load_image $fpga_location 0 {} {} {}\n",
            path_word(&load.file)?,
            load.offset,
            tcl_word(&load.format)
        ));
    }
    tcl.push_str("release_soc\nrun_workload 100000 $wave $wave_start\nexit\n");
    Ok(tcl)
}

pub struct VdbgProcess {
    child: Child,
    exit_flag: Option<PathBuf>,
}

impl Drop for VdbgProcess {
    fn drop(&mut self) {
        if let Some(exit_flag) = &self.exit_flag {
            std::fs::write(exit_flag, "").expect("failed to signal vdbg exit");
            return;
        }
        let process_group = i32::try_from(self.child.id()).expect("vdbg PID must fit pid_t");
        unsafe {
            libc::kill(-process_group, libc::SIGKILL);
        }
        let _ = self.child.wait();
    }
}

impl VdbgProcess {
    pub fn check_running(&mut self) -> Result<(), String> {
        if let Some(status) = self.child.try_wait().map_err(|e| format!("vdbg status: {e}"))? {
            return Err(format!("vdbg exited before the workload completed: {status}"));
        }
        Ok(())
    }

    pub fn exit_on_drop(mut self, exit_flag: PathBuf) -> Self {
        self.exit_flag = Some(exit_flag);
        self
    }
}

pub fn start_vdbg_background(tcl_path: &Path) -> Result<VdbgProcess, String> {
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    let sourceme = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh");
    if !sourceme.exists() {
        return Err(format!("sourceme.sh not found: {}", sourceme.display()));
    }

    log::info!("Starting vdbg: {}", tcl_path.display());

    let child = Command::new("bash")
        .arg("-c")
        .arg("source \"$1\" && exec vdbg \"$2\"")
        .arg("bash")
        .arg(&sourceme)
        .arg(tcl_path)
        .env_remove("LD_PRELOAD")
        .process_group(0)
        .spawn()
        .map_err(|e| format!("Failed to start vdbg: {}", e))?;

    Ok(VdbgProcess { child, exit_flag: None })
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

pub fn wait_for_flash(
    flash_done_flag: &Path,
    vdbg: &mut VdbgProcess,
    mut poll: impl FnMut() -> Result<(), String>,
) -> Result<(), String> {
    loop {
        poll()?;
        if flash_done_flag.exists() {
            return Ok(());
        }
        if let Some(status) = vdbg
            .child
            .try_wait()
            .map_err(|error| format!("failed to query vdbg status: {error}"))?
        {
            return Err(format!("vdbg exited before flash completed: {status}"));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
