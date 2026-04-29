use std::path::{Path, PathBuf};
use std::process::Command;

/// 比特流构建管道
pub struct BitstreamBuilder {
    vvac_top_module: String,
    output_dir: PathBuf,
    hw_config: Option<PathBuf>,
    vcom_tcl: Option<PathBuf>,
}

impl BitstreamBuilder {
    /// 创建新的比特流构建器
    pub fn new() -> Self {
        Self {
            vvac_top_module: String::from("xepic_vvac_top"),
            output_dir: PathBuf::from("./out"),
            hw_config: None,
            vcom_tcl: None,
        }
    }

    /// 设置 VVAC 生成后的顶层模块名
    pub fn vvac_top_module(mut self, name: &str) -> Self {
        self.vvac_top_module = name.to_string();
        self
    }

    /// 设置输出目录
    pub fn output_dir<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.output_dir = path.as_ref().to_path_buf();
        self
    }

    /// 设置硬件配置文件
    pub fn hw_config<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.hw_config = Some(path.as_ref().to_path_buf());
        self
    }

    /// 设置 vcom TCL 脚本
    pub fn vcom_tcl<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.vcom_tcl = Some(path.as_ref().to_path_buf());
        self
    }

    /// 执行完整构建流程
    pub fn build(&self) -> Result<(), String> {
        log::info!("Starting P2E bitstream build...");

        // 创建输出目录
        std::fs::create_dir_all(&self.output_dir)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;

        self.verify_vvac_outputs()?;

        // Step 1: vsyn - 综合
        self.run_vsyn()?;

        // Step 2: vcom - 系统编译
        self.run_vcom()?;

        // Step 3: PNR - 布局布线
        self.run_pnr()?;

        log::info!("P2E bitstream build completed successfully");
        log::info!("Bitstream: {:?}", self.bitstream_path());
        log::info!("libvCtb.so: {:?}", self.libvctb_path());
        Ok(())
    }

    fn verify_vvac_outputs(&self) -> Result<(), String> {
        let vvac_dir = self.output_dir.join("vvacDir");
        if !vvac_dir.exists() {
            return Err(format!(
                "vvacDir not found at {}; build p2e with ARCH_CONFIG first",
                vvac_dir.display()
            ));
        }

        let libvctb = self.libvctb_path();
        if !libvctb.exists() {
            return Err(format!(
                "libvCtb.so not found at {}; build p2e with ARCH_CONFIG first",
                libvctb.display()
            ));
        }

        Ok(())
    }

    /// 运行 vsyn 综合
    fn run_vsyn(&self) -> Result<(), String> {
        log::info!("Running vsyn synthesis...");

        let filelist = self.output_dir.join("vvacDir/vvac_by_mod/filelist");
        if !filelist.exists() {
            return Err(format!("vvac filelist not found: {:?}", filelist));
        }

        let vvac_top = &self.vvac_top_module;
        let output_vm = self.output_dir.join(format!("{}.vm", vvac_top));

        let status = Command::new("vsyn")
            .arg("-F")
            .arg(&filelist)
            .arg("-top")
            .arg(vvac_top)
            .arg("-o")
            .arg(&output_vm)
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
        Ok(())
    }

    /// 运行 vcom 系统编译
    fn run_vcom(&self) -> Result<(), String> {
        log::info!("Running vcom system build...");

        // 设置环境变量
        std::env::set_var("__XEPIC_NEW_NETLIST_MACRO_FLOW", "1");
        std::env::set_var("top_module", &self.vvac_top_module);
        if let Some(ref hw_config) = self.hw_config {
            if let Some(parent) = hw_config.parent() {
                std::env::set_var("PROJECT_DIR", parent);
            }
        }

        // 查找 vcom_compile.tcl
        let vcom_tcl = if let Some(ref tcl) = self.vcom_tcl {
            tcl.clone()
        } else {
            self.find_vcom_tcl()?
        };

        if !vcom_tcl.exists() {
            return Err(format!("vcom TCL script not found: {:?}", vcom_tcl));
        }

        let status = Command::new("vcom")
            .arg(abs_existing(&vcom_tcl)?)
            .current_dir(&self.output_dir)
            .status()
            .map_err(|e| format!("Failed to execute vcom: {}", e))?;

        if !status.success() {
            return Err("vcom system build failed".to_string());
        }

        // 验证生成的 fpgaCompDir
        let fpga_comp_dir = self.output_dir.join("fpgaCompDir");
        if !fpga_comp_dir.exists() {
            return Err(format!("fpgaCompDir not generated: {:?}", fpga_comp_dir));
        }

        log::info!("vcom system build completed");
        log::info!("  FPGA project: {:?}", fpga_comp_dir);
        Ok(())
    }

    /// 运行 PNR 布局布线
    fn run_pnr(&self) -> Result<(), String> {
        log::info!("Running PNR (Place and Route)...");

        let fpga_comp_dir = self.output_dir.join("fpgaCompDir");

        if !fpga_comp_dir.exists() {
            return Err(format!("fpgaCompDir not found: {:?}", fpga_comp_dir));
        }

        let status = Command::new("make")
            .arg("-C")
            .arg(&fpga_comp_dir)
            .arg("all")
            .status()
            .map_err(|e| format!("Failed to execute make: {}", e))?;

        if !status.success() {
            return Err("PNR failed".to_string());
        }

        let bitstream = self.bitstream_path();
        if !bitstream.exists() {
            return Err(format!("Bitstream not generated: {:?}", bitstream));
        }

        log::info!("PNR completed");
        log::info!("  Bitstream: {:?}", bitstream);
        Ok(())
    }

    /// 查找 vcom_compile.tcl
    fn find_vcom_tcl(&self) -> Result<PathBuf, String> {
        // 优先使用用户指定的路径
        if let Some(ref hw_config) = self.hw_config {
            let parent = hw_config.parent().unwrap_or(Path::new("."));
            let tcl = parent.join("vcom_compile.tcl");
            if tcl.exists() {
                return Ok(tcl);
            }
        }

        // 在输出目录查找
        let tcl = self.output_dir.join("vcom_compile.tcl");
        if tcl.exists() {
            return Ok(tcl);
        }

        // 在当前目录查找
        let tcl = PathBuf::from("vcom_compile.tcl");
        if tcl.exists() {
            return Ok(tcl);
        }

        Err("vcom_compile.tcl not found".to_string())
    }

    /// 获取比特流路径
    pub fn bitstream_path(&self) -> PathBuf {
        self.output_dir.join("fpgaCompDir/bitstream.bit")
    }

    /// 获取 libvCtb.so 路径
    pub fn libvctb_path(&self) -> PathBuf {
        self.output_dir
            .join("vvacDir/runtimeDir/lib/lib_arm/libvCtb.so")
    }

    /// 获取运行时配置路径
    pub fn rtcfg_path(&self) -> PathBuf {
        self.output_dir.join("vvacDir/runtimeDir/rtcfg")
    }
}

impl Default for BitstreamBuilder {
    fn default() -> Self {
        Self::new()
    }
}

fn abs_existing(path: &Path) -> Result<PathBuf, String> {
    std::fs::canonicalize(path).map_err(|e| format!("failed to resolve {}: {}", path.display(), e))
}
