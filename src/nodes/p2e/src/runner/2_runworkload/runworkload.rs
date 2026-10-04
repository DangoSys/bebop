use crate::runner::SimulationResult;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Run complete P2E workflow: flash + init + workload
pub struct RunWorkloadStep {
    pub fpga_location: String,
    pub output_dir: PathBuf,
    pub image_path: PathBuf,
    pub bitstream_path: PathBuf,
    pub ddr_channel: u32,
    pub timeout: Duration,
}

impl RunWorkloadStep {
    pub fn new(
        fpga_location: impl Into<String>,
        output_dir: impl Into<PathBuf>,
        image_path: impl Into<PathBuf>,
        bitstream_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            fpga_location: fpga_location.into(),
            output_dir: output_dir.into(),
            image_path: image_path.into(),
            bitstream_path: bitstream_path.into(),
            ddr_channel: 0,
            timeout: Duration::from_secs(300), // 5 minutes default
        }
    }

    pub fn ddr_channel(mut self, channel: u32) -> Self {
        self.ddr_channel = channel;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn run(&self) -> Result<SimulationResult, String> {
        log::info!("Running complete P2E workflow using vdbg...");
        log::info!("  FPGA Location: {}", self.fpga_location);
        log::info!("  Output Dir: {:?}", self.output_dir);
        log::info!("  Image: {:?}", self.image_path);
        log::info!("  DDR Channel: {}", self.ddr_channel);

        // Validate paths
        self.validate_paths()?;

        // Generate main run.tcl script that orchestrates everything
        let tcl_script = self.generate_run_tcl()?;
        let tcl_path = self.output_dir.join("run.tcl");
        std::fs::write(&tcl_path, tcl_script).map_err(|e| format!("Failed to write run.tcl: {}", e))?;

        log::info!("Generated run.tcl: {:?}", tcl_path);

        // Find sourceme.sh
        let sourceme = self.find_sourceme()?;
        log::info!("Using sourceme.sh: {:?}", sourceme);

        // Run vdbg with sourced environment
        // CRITICAL: Use LD_PRELOAD to load Rust DPI-C functions (scu_uart_write, scu_sim_exit)
        // Find libbebop_p2e.so in target/release/deps or vvacDir/runtimeDir/lib/lib_arm
        let bebop_lib = if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
            let release_lib = format!("{}/release/deps/libbebop_p2e.so", target_dir);
            if std::path::Path::new(&release_lib).exists() {
                release_lib
            } else {
                format!("{}/out/vvacDir/runtimeDir/lib/lib_arm/libbebop_p2e.so", target_dir)
            }
        } else if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            let release_lib = format!("{}/target/release/deps/libbebop_p2e.so", manifest_dir);
            if std::path::Path::new(&release_lib).exists() {
                release_lib
            } else {
                format!("{}/out/vvacDir/runtimeDir/lib/lib_arm/libbebop_p2e.so", manifest_dir)
            }
        } else {
            "./vvacDir/runtimeDir/lib/lib_arm/libbebop_p2e.so".to_string()
        };

        log::info!("Using Rust DPI-C library: {}", bebop_lib);

        let cmd = format!(
            "source {} && cd {} && LD_PRELOAD={} vdbg run.tcl",
            sourceme.display(),
            self.output_dir
                .canonicalize()
                .map_err(|e| format!("Failed to canonicalize output_dir: {}", e))?
                .display(),
            bebop_lib
        );

        log::info!("Executing: {}", cmd);

        let start = std::time::Instant::now();
        let status = Command::new("bash")
            .arg("-c")
            .arg(&cmd)
            .status()
            .map_err(|e| format!("Failed to execute vdbg: {}", e))?;

        if !status.success() {
            return Err("vdbg execution failed".to_string());
        }

        let elapsed = start.elapsed();

        log::info!("Workflow completed");
        log::info!("  Elapsed: {:?}", elapsed);

        // Get UART log and exit code from FFI
        let uart_log = crate::ffi::uart_log();
        let exit_code = crate::ffi::exit_code();

        Ok(SimulationResult {
            exit_code,
            elapsed,
            uart_log,
        })
    }

    fn validate_paths(&self) -> Result<(), String> {
        if !self.output_dir.exists() {
            return Err(format!("Output directory not found: {}", self.output_dir.display()));
        }

        if !self.image_path.exists() {
            return Err(format!("Image not found: {}", self.image_path.display()));
        }

        // Check if TCL template files exist
        let flash_tcl = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runner/0_flashbitstream/flash.tcl");
        if !flash_tcl.exists() {
            return Err(format!("flash.tcl template not found: {}", flash_tcl.display()));
        }

        let init_tcl = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runner/1_init/init.tcl");
        if !init_tcl.exists() {
            return Err(format!("init.tcl template not found: {}", init_tcl.display()));
        }

        let workload_tcl = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runner/2_runworkload/workload.tcl");
        if !workload_tcl.exists() {
            return Err(format!("workload.tcl template not found: {}", workload_tcl.display()));
        }

        Ok(())
    }

    fn generate_run_tcl(&self) -> Result<String, String> {
        if self.ddr_channel != 0 {
            return Err("cold-load runner requires DDR channel zero".into());
        }
        let plan = crate::runner::validate_loads(Some(&self.image_path), None)?;
        let bitstream = self
            .bitstream_path
            .canonicalize()
            .map_err(|e| format!("bitstream: {e}"))?;
        let case = bitstream
            .parent()
            .and_then(|p| p.parent())
            .ok_or("invalid bitstream case path")?;
        let output = self.output_dir.canonicalize().map_err(|e| e.to_string())?;
        if case != output.as_path() {
            return Err("bitstream must belong to the cold-load output case".into());
        }
        let record = serde_json::to_vec_pretty(&plan).map_err(|e| e.to_string())?;
        std::fs::write(self.output_dir.join("load-inputs.json"), record).map_err(|e| e.to_string())?;
        crate::runner::generate_main_tcl(&self.fpga_location, &plan, &bitstream, false, false, 0)
    }

    fn find_sourceme(&self) -> Result<PathBuf, String> {
        // Try to find sourceme.sh in common locations
        let candidates = vec![
            PathBuf::from("sourceme.sh"),
            // PathBuf::from("./sourceme.sh"),
            // self.output_dir.join("../sourceme.sh"),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh"),
        ];

        for path in candidates {
            if path.exists() {
                return Ok(path);
            }
        }

        Err("sourceme.sh not found. Please ensure sourceme.sh exists in the project directory".to_string())
    }
}
