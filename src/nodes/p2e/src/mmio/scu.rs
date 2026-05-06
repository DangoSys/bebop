const SIM_EXIT_ADDR: u32 = 0x60000000;
const UART_TX_ADDR: u32 = 0x60020000;

/// SCU (System Control Unit) 控制器
pub struct ScuController;

impl ScuController {
    /// SCU MMIO 写入
    pub fn write(addr: u32, data: u32) -> Result<(), String> {
        let result = crate::ffi::host_mmio_write(addr as u64, data as u64);
        if result == 0 {
            Ok(())
        } else {
            Err(format!("SCU write failed at 0x{:x}", addr))
        }
    }

    /// SCU MMIO 读取
    pub fn read(addr: u32) -> u32 {
        crate::ffi::host_mmio_read(addr as u64) as u32
    }

    /// UART 输出单个字符
    pub fn uart_putc(ch: u8) -> Result<(), String> {
        Self::write(UART_TX_ADDR, ch as u32)
    }

    /// UART 输出字符串
    pub fn uart_puts(s: &str) -> Result<(), String> {
        for ch in s.bytes() {
            Self::uart_putc(ch)?;
        }
        Ok(())
    }

    /// 请求退出仿真
    pub fn request_exit(code: i32) -> Result<(), String> {
        Self::write(SIM_EXIT_ADDR, code as u32)
    }
}
