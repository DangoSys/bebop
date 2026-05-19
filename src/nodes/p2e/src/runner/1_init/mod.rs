use std::path::{Path, PathBuf};
use std::process::Command;

/// Initialize FPGA and DDR4 memory
pub struct InitStep {
    pub fpga_comp_dir: PathBuf,
    pub init_timeout_ms: u64,
    pub fpga_location: Option<String>,
    pub image_path: Option<PathBuf>,
}

impl InitStep {
    pub fn new(fpga_comp_dir: impl Into<PathBuf>) -> Self {
        Self {
            fpga_comp_dir: fpga_comp_dir.into(),
            init_timeout_ms: 30000, // 30 seconds default
            fpga_location: None,
            image_path: None,
        }
    }

    pub fn timeout(mut self, timeout_ms: u64) -> Self {
        self.init_timeout_ms = timeout_ms;
        self
    }

    pub fn fpga_location(mut self, location: impl Into<String>) -> Self {
        self.fpga_location = Some(location.into());
        self
    }

    pub fn image(mut self, path: impl Into<PathBuf>) -> Self {
        self.image_path = Some(path.into());
        self
    }

    pub fn run(&self) -> Result<(), String> {
        log::info!("Initializing FPGA and DDR4...");
        log::info!("  FPGA project: {:?}", self.fpga_comp_dir);

        if !self.fpga_comp_dir.exists() {
            return Err(format!(
                "FPGA project directory not found: {}",
                self.fpga_comp_dir.display()
            ));
        }

        // Copy init.tcl from source directory to output directory
        let init_tcl_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runner/1_init/init.tcl");
        let init_tcl_dst = self.fpga_comp_dir.join("init.tcl");

        if !init_tcl_src.exists() {
            return Err(format!("init.tcl not found: {}", init_tcl_src.display()));
        }

        std::fs::copy(&init_tcl_src, &init_tcl_dst).map_err(|e| format!("Failed to copy init.tcl: {}", e))?;

        log::info!("Running initialization script...");
        self.run_init_script(&init_tcl_dst)?;

        log::info!("Loading image to DDR...");
        if let (Some(fpga_location), Some(image_path)) = (&self.fpga_location, &self.image_path) {
            self.load_image(fpga_location, image_path)?;
        }

        log::info!("Initialization completed successfully");
        Ok(())
    }

    fn run_init_script(&self, tcl_path: &PathBuf) -> Result<(), String> {
        let sourceme = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh");
        if !sourceme.exists() {
            return Err(format!("sourceme.sh not found: {}", sourceme.display()));
        }

        // Use vdbg instead of vivado to run the init script
        // vdbg will stay running and CTB can connect to it
        let cmd = format!(
            "source {} && cd {} && vdbg -mode batch -source {}",
            sourceme.display(),
            self.fpga_comp_dir.display(),
            tcl_path.display()
        );

        log::info!("Executing: {}", cmd);

        // CRITICAL: Clear LD_PRELOAD to avoid glibc version conflicts
        // LD_PRELOAD contains libbebop_p2e.so which is compiled with newer glibc
        // and will cause vdbg/bash to fail with "GLIBC_X.XX not found" errors
        let status = Command::new("bash")
            .arg("-c")
            .arg(&cmd)
            .env_remove("LD_PRELOAD")
            .status()
            .map_err(|e| format!("Failed to execute init script: {}", e))?;

        if !status.success() {
            return Err("Initialization script failed".to_string());
        }

        Ok(())
    }

    fn load_image(&self, fpga_location: &str, image: &Path) -> Result<(), String> {
        log::info!("Loading image to DDR via vdbg...");

        if !image.exists() {
            return Err(format!("Image not found: {}", image.display()));
        }

        let sourceme = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh");
        if !sourceme.exists() {
            return Err(format!("sourceme.sh not found: {}", sourceme.display()));
        }

        // Generate TCL script to load image
        let tcl_script = format!(
            r#"
# Load workload.tcl
source {}/workload.tcl

# Call load_image function
load_image {} 0 {}

exit
"#,
            self.fpga_comp_dir.join("../src/runner/2_runworkload").display(),
            fpga_location,
            image.display()
        );

        let tcl_path = self.fpga_comp_dir.join("load_image.tcl");
        std::fs::write(&tcl_path, tcl_script).map_err(|e| format!("Failed to write load_image.tcl: {}", e))?;

        // Run vdbg to execute the TCL script
        let cmd = format!(
            "source {} && cd {} && vdbg load_image.tcl",
            sourceme.display(),
            self.fpga_comp_dir.display()
        );

        log::info!("Executing: {}", cmd);

        // CRITICAL: Clear LD_PRELOAD to avoid glibc version conflicts
        // LD_PRELOAD contains libbebop_p2e.so which is compiled with newer glibc
        // and will cause bash/vdbg to fail with "GLIBC_X.XX not found" errors
        let status = Command::new("bash")
            .arg("-c")
            .arg(&cmd)
            .env_remove("LD_PRELOAD")
            .status()
            .map_err(|e| format!("Failed to execute vdbg: {}", e))?;

        if !status.success() {
            return Err("vdbg load_image failed".to_string());
        }

        log::info!("Image loaded successfully");
        Ok(())
    }
}
