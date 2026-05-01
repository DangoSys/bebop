use crate::runner::{P2ESimulator, SimulationResult, SimulatorConfig};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Run workload on P2E FPGA
pub struct RunWorkloadStep {
    pub fpga_id: String,
    pub case_home: PathBuf,
    pub rtcfg_path: PathBuf,
    pub binary_path: PathBuf,
    pub timeout: Duration,
}

impl RunWorkloadStep {
    pub fn new(
        fpga_id: impl Into<String>,
        case_home: impl Into<PathBuf>,
        rtcfg_path: impl Into<PathBuf>,
        binary_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            fpga_id: fpga_id.into(),
            case_home: case_home.into(),
            rtcfg_path: rtcfg_path.into(),
            binary_path: binary_path.into(),
            timeout: Duration::from_secs(300), // 5 minutes default
        }
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn run(&self) -> Result<SimulationResult, String> {
        log::info!("Running workload on P2E FPGA...");
        log::info!("  FPGA ID: {}", self.fpga_id);
        log::info!("  Case home: {:?}", self.case_home);
        log::info!("  Runtime config: {:?}", self.rtcfg_path);
        log::info!("  Binary: {:?}", self.binary_path);

        // Validate paths
        self.validate_paths()?;

        // Load binary to memory
        self.load_binary()?;

        // Create simulator
        let config = SimulatorConfig::new(
            &self.fpga_id,
            self.case_home.to_str().ok_or("Invalid case_home path")?,
            self.rtcfg_path.to_str().ok_or("Invalid rtcfg_path")?,
        )
        .step_cycles(1000)
        .poll_interval(Duration::from_millis(10));

        let mut sim = P2ESimulator::with_config(config)?;

        // Reset and run
        log::info!("Resetting FPGA...");
        sim.reset()?;

        log::info!("Starting simulation...");
        let result = sim.run_until_exit()?;

        log::info!("Simulation completed");
        log::info!("  Exit code: {}", result.exit_code);
        log::info!("  Elapsed: {:?}", result.elapsed);
        log::info!("  Cycles: {}", result.cycles);

        if !result.uart_log.is_empty() {
            log::info!("UART output:");
            for line in result.uart_log.lines() {
                log::info!("  {}", line);
            }
        }

        Ok(result)
    }

    fn validate_paths(&self) -> Result<(), String> {
        if !self.case_home.exists() {
            return Err(format!("Case home not found: {}", self.case_home.display()));
        }

        if !self.rtcfg_path.exists() {
            return Err(format!("Runtime config not found: {}", self.rtcfg_path.display()));
        }

        if !self.binary_path.exists() {
            return Err(format!("Binary not found: {}", self.binary_path.display()));
        }

        Ok(())
    }

    fn load_binary(&self) -> Result<(), String> {
        log::info!("Loading binary to memory...");

        // Read binary file
        let binary_data = std::fs::read(&self.binary_path)
            .map_err(|e| format!("Failed to read binary: {}", e))?;

        log::info!("  Binary size: {} bytes", binary_data.len());

        // TODO: Implement actual binary loading via DMA or MMIO
        // This depends on your memory backdoor implementation
        // For now, just log that we would load it
        log::warn!("Binary loading not fully implemented yet");

        Ok(())
    }
}
