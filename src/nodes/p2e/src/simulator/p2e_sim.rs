use crate::ffi::{self, CtbManager};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct SimulatorConfig {
    pub fpga_id: String,
    pub case_home: PathBuf,
    pub rtcfg_path: PathBuf,
    pub step_cycles: u32,
    pub poll_interval: Duration,
}

impl SimulatorConfig {
    pub fn new(
        fpga_id: impl Into<String>,
        case_home: impl Into<PathBuf>,
        rtcfg_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            fpga_id: fpga_id.into(),
            case_home: case_home.into(),
            rtcfg_path: rtcfg_path.into(),
            step_cycles: 1000,
            poll_interval: Duration::from_millis(10),
        }
    }

    pub fn step_cycles(mut self, cycles: u32) -> Self {
        self.step_cycles = cycles.max(1);
        self
    }

    pub fn poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }
}

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub exit_code: i32,
    pub elapsed: Duration,
    pub cycles: u64,
    pub timed_out: bool,
    pub uart_log: String,
}

/// P2E simulator backed by VVAC CTB. The public control surface mirrors the
/// Verilator runner: initialize, reset, step, then run until an MMIO exit.
pub struct P2ESimulator {
    _ctb: CtbManager,
    config: SimulatorConfig,
    cycles: u64,
}

impl P2ESimulator {
    pub fn new(fpga_id: &str, case_home: &str, rtcfg_path: &str) -> Result<Self, String> {
        Self::with_config(SimulatorConfig::new(fpga_id, case_home, rtcfg_path))
    }

    pub fn with_config(config: SimulatorConfig) -> Result<Self, String> {
        configure_vvac_environment();
        ffi::reset_runtime_state();

        validate_path(&config.case_home, "case_home")?;
        validate_path(&config.rtcfg_path, "rtcfg_path")?;

        let ctb = CtbManager::new()?;
        ctb.init(
            &config.fpga_id,
            path_to_str(&config.case_home)?,
            path_to_str(&config.rtcfg_path)?,
        )?;

        ffi::mark_initialized();

        Ok(Self {
            _ctb: ctb,
            config,
            cycles: 0,
        })
    }

    pub fn reset(&mut self) -> Result<(), String> {
        self.step(10)
    }

    pub fn step(&mut self, cycles: u32) -> Result<(), String> {
        if cycles == 0 {
            return Ok(());
        }
        ffi::wait_cycles(cycles)?;
        self.cycles += cycles as u64;
        Ok(())
    }

    pub fn run_until_exit(&mut self) -> Result<i32, String> {
        let result = self.run_until_exit_timeout(None)?;
        Ok(result.exit_code)
    }

    pub fn run_for(&mut self, seconds: u64) -> Result<SimulationResult, String> {
        self.run_until_exit_timeout(Some(Duration::from_secs(seconds)))
    }

    pub fn run_until_exit_timeout(
        &mut self,
        timeout: Option<Duration>,
    ) -> Result<SimulationResult, String> {
        let started = Instant::now();

        loop {
            if self.check_exit() {
                return Ok(self.result(started, false));
            }

            if timeout
                .map(|limit| started.elapsed() >= limit)
                .unwrap_or(false)
            {
                return Ok(self.result(started, true));
            }

            self.step(self.config.step_cycles)?;

            if !self.config.poll_interval.is_zero() {
                std::thread::sleep(self.config.poll_interval);
            }
        }
    }

    pub fn check_exit(&self) -> bool {
        ffi::check_exit()
    }

    pub fn get_exit_code(&self) -> i32 {
        ffi::exit_code()
    }

    pub fn get_uart_log(&self) -> String {
        ffi::uart_log()
    }

    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    fn result(&self, started: Instant, timed_out: bool) -> SimulationResult {
        SimulationResult {
            exit_code: self.get_exit_code(),
            elapsed: started.elapsed(),
            cycles: self.cycles,
            timed_out,
            uart_log: self.get_uart_log(),
        }
    }
}

fn configure_vvac_environment() {
    std::env::set_var("VMRI_LOG_LEVEL", env_default("VMRI_LOG_LEVEL", "0"));
    std::env::set_var("VVAC_LOG_LEVEL", env_default("VVAC_LOG_LEVEL", "0"));
    std::env::set_var("RBMGR_LOG_LEVEL", env_default("RBMGR_LOG_LEVEL", "0"));
    std::env::set_var("RBMGR_DUMP_DATA", env_default("RBMGR_DUMP_DATA", "1"));
    std::env::set_var("RTL_DBG_SIZE", env_default("RTL_DBG_SIZE", "128"));

    if std::env::var("VCOM_TEST_DIP").is_ok() {
        std::env::set_var("VMRI_WORK_MODE", env_default("VMRI_WORK_MODE", "4"));
        std::env::set_var("VVAC_WORK_MODE", env_default("VVAC_WORK_MODE", "1"));
        log::info!("Running P2E in VVAC simulation mode");
    } else {
        std::env::set_var("VMRI_WORK_MODE", env_default("VMRI_WORK_MODE", "3"));
        std::env::set_var("VVAC_WORK_MODE", env_default("VVAC_WORK_MODE", "0"));
        log::info!("Running P2E in onboard mode");
    }
}

fn env_default(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn validate_path(path: &Path, name: &str) -> Result<(), String> {
    if path.exists() {
        Ok(())
    } else {
        Err(format!("{} not found: {}", name, path.display()))
    }
}

fn path_to_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))
}
