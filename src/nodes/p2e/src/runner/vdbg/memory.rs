use std::process::Command;

/// DDR 后门访问（通过 vdbg TCL）
pub struct MemoryBackdoor;

impl MemoryBackdoor {
    pub fn new() -> Self {
        Self
    }

    /// DDR backdoor 写入
    ///
    /// # Arguments
    /// * `fpga_id` - FPGA ID (如 "0.A")
    /// * `addr` - 起始地址
    /// * `file` - 数据文件路径
    pub fn write(&self, fpga_id: &str, addr: u64, file: &str) -> Result<(), String> {
        let tcl = format!(
            "memory -write -fpga {} -channel 0 -file {} -start {:#x}",
            fpga_id, file, addr
        );

        self.exec_vdbg_tcl(&tcl)?;
        log::debug!("Memory write: 0x{:x} from {}", addr, file);
        Ok(())
    }

    /// DDR backdoor 读取
    ///
    /// # Arguments
    /// * `fpga_id` - FPGA ID (如 "0.A")
    /// * `addr` - 起始地址
    /// * `size` - 读取大小（字节）
    /// * `output` - 输出文件路径
    pub fn read(&self, fpga_id: &str, addr: u64, size: usize, output: &str) -> Result<(), String> {
        let end_addr = addr + size as u64 - 1;
        let tcl = format!(
            "memory -read -fpga {} -channel 0 -file {} -start {:#x} -end {:#x}",
            fpga_id, output, addr, end_addr
        );

        self.exec_vdbg_tcl(&tcl)?;
        log::debug!("Memory read: 0x{:x} ({} bytes) to {}", addr, size, output);
        Ok(())
    }

    /// 执行 vdbg TCL 命令
    fn exec_vdbg_tcl(&self, tcl: &str) -> Result<(), String> {
        let output = Command::new("vdbg")
            .arg("-eval")
            .arg(tcl)
            .output()
            .map_err(|e| format!("vdbg execution failed: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("vdbg command failed: {}", stderr));
        }

        Ok(())
    }
}
