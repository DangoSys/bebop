use super::tcl_gen::TclGenerator;
use std::path::{Path, PathBuf};
use std::process::Command;

/// vdbg 会话管理器
pub struct VdbgSession {
    bitstream: PathBuf,
    hw_config: PathBuf,
    fpga_id: String,
    hw_server: String,
    work_dir: PathBuf,
}

impl VdbgSession {
    /// 创建新的 vdbg 会话
    pub fn new() -> Self {
        Self {
            bitstream: PathBuf::new(),
            hw_config: PathBuf::new(),
            fpga_id: "0.A".to_string(),
            hw_server: ".".to_string(),
            work_dir: PathBuf::from("."),
        }
    }

    /// 设置比特流路径
    pub fn bitstream<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.bitstream = path.as_ref().to_path_buf();
        self
    }

    /// 设置硬件配置文件
    pub fn hw_config<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.hw_config = path.as_ref().to_path_buf();
        self
    }

    /// 设置 FPGA ID
    pub fn fpga_id(mut self, id: &str) -> Self {
        self.fpga_id = id.to_string();
        self
    }

    /// 设置硬件服务器
    pub fn hw_server(mut self, server: &str) -> Self {
        self.hw_server = server.to_string();
        self
    }

    /// 设置工作目录
    pub fn work_dir<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.work_dir = path.as_ref().to_path_buf();
        self
    }

    /// 连接到 FPGA
    pub fn connect(&self) -> Result<(), String> {
        log::info!("Connecting to FPGA {}...", self.fpga_id);

        let tcl = self.tcl_prelude().build();

        self.exec_tcl(&tcl)?;
        log::info!("Connected to FPGA");
        Ok(())
    }

    /// 下载比特流
    pub fn download_bitstream(&self) -> Result<(), String> {
        log::info!("Downloading bitstream...");

        if !self.bitstream.exists() {
            return Err(format!("Bitstream not found: {:?}", self.bitstream));
        }

        let tcl = self.tcl_prelude().download().build();

        self.exec_tcl(&tcl)?;
        log::info!("Bitstream downloaded");
        Ok(())
    }

    /// 初始化硬件
    pub fn init_hardware(&self) -> Result<(), String> {
        log::info!("Initializing hardware...");

        let tcl = self
            .tcl_prelude()
            .force("user_rst", "1")
            .run_cycles(100, "rclk")
            .force("user_rst", "0")
            .run_cycles(100, "rclk")
            .build();

        self.exec_tcl(&tcl)?;
        log::info!("Hardware initialized");
        Ok(())
    }

    /// DDR backdoor 写入
    pub fn memory_write(&mut self, addr: u64, file: &str) -> Result<(), String> {
        log::info!("Writing memory at 0x{:x} from {}", addr, file);

        if !Path::new(file).exists() {
            return Err(format!("File not found: {}", file));
        }

        let file = std::fs::canonicalize(file).map_err(|e| format!("Failed to resolve memory input file: {}", e))?;
        let tcl = self.tcl_prelude().memory_write(&self.fpga_id, addr, &file).build();
        self.exec_tcl(&tcl)?;
        Ok(())
    }

    /// DDR backdoor 读取
    pub fn memory_read(&self, addr: u64, size: usize, output: &str) -> Result<(), String> {
        log::info!("Reading memory at 0x{:x} ({} bytes) to {}", addr, size, output);

        let tcl = self
            .tcl_prelude()
            .memory_read(&self.fpga_id, addr, size, output)
            .build();
        self.exec_tcl(&tcl)?;
        Ok(())
    }

    /// 启动 DPI-C 服务
    pub fn start_dpic_server(&self) -> Result<(), String> {
        log::info!("DPI-C server ready (managed by vdbg)");
        Ok(())
    }

    /// 断开连接
    pub fn disconnect(&self) -> Result<(), String> {
        log::info!("Disconnecting from FPGA...");

        let tcl = "exit";
        self.exec_tcl(tcl)?;

        log::info!("Disconnected");
        Ok(())
    }

    /// 执行 TCL 命令
    fn exec_tcl(&self, tcl: &str) -> Result<(), String> {
        // 创建临时 TCL 文件
        let tcl_file = self.work_dir.join("temp.tcl");
        std::fs::write(&tcl_file, tcl).map_err(|e| format!("Failed to write TCL file: {}", e))?;

        // 执行 vdbg
        let output = Command::new("vdbg")
            .arg(tcl_file.to_str().unwrap())
            .current_dir(&self.work_dir)
            .output()
            .map_err(|e| format!("Failed to execute vdbg: {}", e))?;

        // 清理临时文件
        let _ = std::fs::remove_file(&tcl_file);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("vdbg failed: {}", stderr));
        }

        Ok(())
    }

    fn tcl_prelude(&self) -> TclGenerator {
        TclGenerator::new().design(&self.work_dir).hw_server(&self.hw_server)
    }
}

impl Default for VdbgSession {
    fn default() -> Self {
        Self::new()
    }
}
