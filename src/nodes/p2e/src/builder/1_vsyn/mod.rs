use std::path::PathBuf;
use std::process::Command;

/// vsyn 综合步骤
pub struct VsynStep {
    /// 设计构建目录（包含 vvacDir）
    pub build_dir: PathBuf,
    /// 输出目录
    pub output_dir: PathBuf,
    /// VVAC 顶层模块名
    pub vvac_top_module: String,
}

impl VsynStep {
    pub fn new(build_dir: PathBuf, output_dir: PathBuf, vvac_top_module: String) -> Self {
        Self {
            build_dir,
            output_dir,
            vvac_top_module,
        }
    }

    /// 运行 vsyn 综合
    pub fn run(&self) -> Result<PathBuf, String> {
        log::info!("Running vsyn synthesis...");

        let filelist = self.build_dir.join("vvacDir/vvac_by_mod/filelist");
        if !filelist.exists() {
            return Err(format!("vvac filelist not found: {:?}", filelist));
        }

        let output_vm = self.output_dir.join(format!("{}.vm", self.vvac_top_module));

        let abs_filelist =
            std::fs::canonicalize(&filelist).map_err(|e| format!("Failed to resolve filelist path: {}", e))?;
        let abs_output_vm = self
            .output_dir
            .canonicalize()
            .map_err(|e| format!("Failed to resolve output dir: {}", e))?
            .join(format!("{}.vm", self.vvac_top_module));

        // Source sourceme.sh and run vsyn
        let sourceme_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh");
        let vsyn_cmd = format!(
            "source {} && vsyn -F {} -top {} -o {}",
            sourceme_path.display(),
            abs_filelist.display(),
            self.vvac_top_module,
            abs_output_vm.display()
        );

        let status = Command::new("bash")
            .arg("-c")
            .arg(&vsyn_cmd)
            .current_dir(&self.output_dir)
            .status()
            .map_err(|e| format!("Failed to execute vsyn: {}", e))?;

        if !status.success() {
            return Err("vsyn synthesis failed".to_string());
        }

        if !output_vm.exists() {
            return Err(format!("Netlist not generated: {:?}", output_vm));
        }

        log::info!("vsyn synthesis completed");
        log::info!("  Netlist: {:?}", output_vm);
        Ok(output_vm)
    }
}
