use std::path::Path;

/// TCL 脚本生成器
pub struct TclGenerator {
    commands: Vec<String>,
}

#[allow(dead_code)]
impl TclGenerator {
    pub fn new() -> Self {
        Self { commands: Vec::new() }
    }

    /// 设置设计目录
    pub fn design<P: AsRef<Path>>(mut self, path: P) -> Self {
        let path_str = path.as_ref().display();
        self.commands.push(format!("design {}", path_str));
        self
    }

    /// 设置硬件服务器
    pub fn hw_server(mut self, server: &str) -> Self {
        self.commands.push(format!("hw_server {}", server));
        self
    }

    /// 设置 PHC 电压
    pub fn set_phc_vol(mut self, id: &str, bank: &str, voltage: f32) -> Self {
        self.commands
            .push(format!("set_phc_vol -id {} -bank {} -voltage {}", id, bank, voltage));
        self
    }

    /// 下载比特流
    pub fn download(mut self) -> Self {
        self.commands.push("download".to_string());
        self
    }

    /// DDR backdoor 写入
    pub fn memory_write<P: AsRef<Path>>(mut self, fpga_id: &str, addr: u64, file: P) -> Self {
        self.commands.push(format!(
            "memory -write -fpga {} -channel 0 -file {} -start {:#x}",
            fpga_id,
            file.as_ref().display(),
            addr
        ));
        self
    }

    /// DDR backdoor 读取
    pub fn memory_read<P: AsRef<Path>>(mut self, fpga_id: &str, addr: u64, size: usize, output: P) -> Self {
        let end_addr = addr + size as u64 - 1;
        self.commands.push(format!(
            "memory -read -fpga {} -channel 0 -file {} -start {:#x} -end {:#x}",
            fpga_id,
            output.as_ref().display(),
            addr,
            end_addr
        ));
        self
    }

    /// 强制信号值
    pub fn force(mut self, signal: &str, value: &str) -> Self {
        self.commands.push(format!("force {} {}", signal, value));
        self
    }

    /// 运行时钟周期
    pub fn run_cycles(mut self, cycles: u32, clock: &str) -> Self {
        self.commands.push(format!("run {}{}", cycles, clock));
        self
    }

    /// 运行（非阻塞）
    pub fn run_nowait(mut self) -> Self {
        self.commands.push("run -nowait".to_string());
        self
    }

    /// 等待
    pub fn after(mut self, ms: u32) -> Self {
        self.commands.push(format!("after {}", ms));
        self
    }

    /// 停止
    pub fn stop(mut self) -> Self {
        self.commands.push("stop".to_string());
        self
    }

    /// 获取信号值
    pub fn get_value(mut self, signal: &str, var: &str) -> Self {
        self.commands.push(format!("set {} [get_value {}]", var, signal));
        self
    }

    /// 打印
    pub fn puts(mut self, msg: &str) -> Self {
        self.commands.push(format!("puts \"{}\"", msg));
        self
    }

    /// 退出
    pub fn exit(mut self) -> Self {
        self.commands.push("exit".to_string());
        self
    }

    /// 构建 TCL 脚本
    pub fn build(self) -> String {
        self.commands.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tcl_generator() {
        let tcl = TclGenerator::new()
            .design(".")
            .hw_server(".")
            .download()
            .force("user_rst", "1")
            .run_cycles(100, "rclk")
            .force("user_rst", "0")
            .exit()
            .build();

        assert!(tcl.contains("design ."));
        assert!(tcl.contains("hw_server ."));
        assert!(tcl.contains("download"));
        assert!(tcl.contains("force user_rst 1"));
        assert!(tcl.contains("run 100rclk"));
    }
}
