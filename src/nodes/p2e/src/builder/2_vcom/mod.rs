use std::path::PathBuf;
use std::process::Command;

/// vcom 系统编译步骤
pub struct VcomStep {
    /// 输出目录
    pub output_dir: PathBuf,
    /// VVAC 顶层模块名
    pub vvac_top_module: String,
    /// vcom_compile.tcl 路径（必须提供）
    pub vcom_tcl: PathBuf,
}

impl VcomStep {
    pub fn new(output_dir: PathBuf, vvac_top_module: String, vcom_tcl: PathBuf) -> Result<Self, String> {
        if !vcom_tcl.exists() {
            return Err(format!("vcom_compile.tcl not found: {:?}", vcom_tcl));
        }

        Ok(Self {
            output_dir,
            vvac_top_module,
            vcom_tcl,
        })
    }

    /// 运行 vcom 系统编译
    pub fn run(&self) -> Result<PathBuf, String> {
        log::info!("Running vcom system build...");

        let abs_tcl = std::fs::canonicalize(&self.vcom_tcl)
            .map_err(|e| format!("Failed to resolve vcom_tcl path: {}", e))?;

        // Source sourceme.sh and run vcom
        let sourceme_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh");
        let vcom_cmd = format!(
            "source {} && export __XEPIC_NEW_NETLIST_MACRO_FLOW=1 && export top_module={} && vcom {}",
            sourceme_path.display(),
            self.vvac_top_module,
            abs_tcl.display()
        );

        let status = Command::new("bash")
            .arg("-c")
            .arg(&vcom_cmd)
            .current_dir(&self.output_dir)
            .status()
            .map_err(|e| format!("Failed to execute vcom: {}", e))?;

        if !status.success() {
            return Err("vcom system build failed".to_string());
        }

        let fpga_comp_dir = self.output_dir.join("fpgaCompDir");
        if !fpga_comp_dir.exists() {
            return Err(format!("fpgaCompDir not generated: {:?}", fpga_comp_dir));
        }

        log::info!("vcom system build completed");
        log::info!("  FPGA project: {:?}", fpga_comp_dir);
        Ok(fpga_comp_dir)
    }
}
