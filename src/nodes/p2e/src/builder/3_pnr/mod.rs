use std::path::PathBuf;
use std::process::Command;

/// PNR 布局布线步骤
pub struct PnrStep {
    /// 输出目录
    pub output_dir: PathBuf,
}

impl PnrStep {
    pub fn new(output_dir: PathBuf) -> Self {
        Self { output_dir }
    }

    /// 运行 PNR 布局布线
    pub fn run(&self) -> Result<PathBuf, String> {
        log::info!("Running PNR (Place and Route)...");

        let fpga_comp_dir = self.output_dir.join("fpgaCompDir");

        if !fpga_comp_dir.exists() {
            return Err(format!("fpgaCompDir not found: {:?}", fpga_comp_dir));
        }

        // Source sourceme.sh and run make
        let sourceme_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh");
        let make_cmd = format!(
            "source {} && make -C {} all",
            sourceme_path.display(),
            fpga_comp_dir.display()
        );

        let status = Command::new("bash")
            .arg("-c")
            .arg(&make_cmd)
            .current_dir(&self.output_dir)
            .status()
            .map_err(|e| format!("Failed to execute make: {}", e))?;

        if !status.success() {
            return Err("PNR failed".to_string());
        }

        let bitstream = self.output_dir.join("fpgaCompDir/bitstream.bit");
        if !bitstream.exists() {
            return Err(format!("Bitstream not generated: {:?}", bitstream));
        }

        log::info!("PNR completed");
        log::info!("  Bitstream: {:?}", bitstream);
        Ok(bitstream)
    }
}
