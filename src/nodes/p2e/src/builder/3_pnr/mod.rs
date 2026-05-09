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

        // Copy PNR_settings.tcl to output directory
        let pnr_settings_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/builder/3_pnr/PNR_settings.tcl");
        let pnr_settings_dst = self.output_dir.join("PNR_settings.tcl");

        if pnr_settings_src.exists() {
            std::fs::copy(&pnr_settings_src, &pnr_settings_dst)
                .map_err(|e| format!("Failed to copy PNR_settings.tcl: {}", e))?;
            log::info!("Copied PNR_settings.tcl to output directory");
        } else {
            log::warn!("PNR_settings.tcl not found in source directory");
        }

        // Source sourceme.sh and run make
        let sourceme_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh");
        let make_cmd = format!(
            "cd {} && source {} && make -C fpgaCompDir clean && make -C fpgaCompDir all",
            self.output_dir.display(),
            sourceme_path.display(),
        );

        let status = Command::new("bash")
            .arg("-c")
            .arg(&make_cmd)
            .status()
            .map_err(|e| format!("Failed to execute make: {}", e))?;

        if !status.success() {
            return Err("PNR failed".to_string());
        }

        // Copy bitstream from pnrDir to fpgaCompDir root
        let bitstream_src = self
            .output_dir
            .join("fpgaCompDir/part_b0_f0/pnrDir/xepic_vvac_top_0_0.bit");
        let bitstream_dst = self.output_dir.join("fpgaCompDir/bitstream.bit");

        if !bitstream_src.exists() {
            return Err(format!("Bitstream not generated: {:?}", bitstream_src));
        }

        std::fs::copy(&bitstream_src, &bitstream_dst).map_err(|e| format!("Failed to copy bitstream: {}", e))?;

        log::info!("PNR completed");
        log::info!("  Bitstream: {:?}", bitstream_dst);
        Ok(bitstream_dst)
    }
}
