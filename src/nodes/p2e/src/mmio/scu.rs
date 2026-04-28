use crate::ffi;

const SIM_EXIT_ADDR: u32 = 0x60000000;
const UART_TX_ADDR: u32 = 0x60020000;

pub struct ScuController;

impl ScuController {
    pub fn write(addr: u32, data: u32) -> Result<(), String> {
        let result = unsafe { ffi::scu_mmio_write(addr, data) };
        if result == 0 {
            Ok(())
        } else {
            Err(format!("SCU write failed at 0x{:x}", addr))
        }
    }

    pub fn read(addr: u32) -> u32 {
        unsafe { ffi::scu_mmio_read(addr) }
    }

    pub fn uart_putc(ch: u8) -> Result<(), String> {
        Self::write(UART_TX_ADDR, ch as u32)
    }

    pub fn uart_puts(s: &str) -> Result<(), String> {
        for ch in s.bytes() {
            Self::uart_putc(ch)?;
        }
        Ok(())
    }

    pub fn request_exit(code: i32) -> Result<(), String> {
        Self::write(SIM_EXIT_ADDR, code as u32)
    }
}
