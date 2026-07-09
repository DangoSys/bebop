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

        // Copy bitstream from pnrDir to fpgaCompDir root
        let bitstream_src = self
            .output_dir
            .join("fpgaCompDir/part_b0_f0/pnrDir/xepic_vvac_top_0_0.bit");
        let bitstream_dst = self.output_dir.join("fpgaCompDir/bitstream.bit");

        if !bitstream_src.exists() {
            if !status.success() {
                return Err("PNR failed".to_string());
            }
            return Err(format!("Bitstream not generated: {:?}", bitstream_src));
        }

        if !status.success() {
            log::warn!("PNR command returned non-zero, but bitstream was generated; continuing");
        }

        std::fs::copy(&bitstream_src, &bitstream_dst).map_err(|e| format!("Failed to copy bitstream: {}", e))?;

        //===----------------------------------------------------------------------===//
        // When the design is hard to PNR, the PNR will fail and generate a bitstream in pnrDir_smart/
        // But the tool still search this file in pnrDir/, so we need to copy it to pnrDir/
        // This is a bug of the xepic tool.
        // The step below is to solve the bug when the design is hard to PNR.
        //
        // Note: This approach is not ideal, as the PNR is highly likely to fail in the end.
        // Although it runs in simulation, this masks the problem; if the PNR fails,
        // you should review the PNR report and make the necessary design modifications.
        //===----------------------------------------------------------------------===//
        // Copy bin file from pnrDir_smart to pnrDir for vdbg compatibility
        // vdbg's download command looks for bin file in pnrDir/, but smart PNR generates it in pnrDir_smart/
        let bin_src = self
            .output_dir
            .join("fpgaCompDir/part_b0_f0/pnrDir_smart/xepic_vvac_top_0_0.bin");
        let bin_dst = self
            .output_dir
            .join("fpgaCompDir/part_b0_f0/pnrDir/xepic_vvac_top_0_0.bin");

        if bin_src.exists() {
            std::fs::copy(&bin_src, &bin_dst).map_err(|e| format!("Failed to copy bin file to pnrDir: {}", e))?;
            log::info!("Copied bin file to pnrDir for vdbg compatibility");
        } else {
            log::warn!("Bin file not found in pnrDir_smart: {:?}", bin_src);
        }

        // Generate RTDB directory for vdbg
        // vdbg needs RTDB/ directory with design database files
        log::info!("Generating RTDB directory for vdbg...");
        let dbg_gen_cmd = format!(
            "cd {dir} && source {sourceme} && export WORK_PATH={dir} && export MEMORYFILEPATH={dir}/ && $VDBG_HOME/tools/vdbg/generateDataFiles.sh",
            dir = self.output_dir.display(),
            sourceme = sourceme_path.display(),
        );

        let status = Command::new("bash")
            .arg("-c")
            .arg(&dbg_gen_cmd)
            .status()
            .map_err(|e| format!("Failed to execute dbgGen: {}", e))?;

        if !status.success() {
            log::warn!("dbgGen failed, but continuing (RTDB may be incomplete)");
        } else {
            log::info!("RTDB directory generated successfully");
        }

        log::info!("PNR completed");
        log::info!("  Bitstream: {:?}", bitstream_dst);
        Ok(bitstream_dst)
    }
}
