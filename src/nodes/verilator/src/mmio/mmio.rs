// MMIO handling

use crate::ffi::VerilatorTop;
use bebop_uart::Uart;
use std::io;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

const SIM_EXIT_ADDR: u64 = 0x60000000;
const UART_BASE_ADDR: u64 = 0x60020000;

static UART: OnceLock<Mutex<Uart>> = OnceLock::new();
static PREV_FIRE: OnceLock<Mutex<u8>> = OnceLock::new();

fn get_uart() -> &'static Mutex<Uart> {
    UART.get_or_init(|| Mutex::new(Uart::new()))
}

fn get_prev_fire() -> &'static Mutex<u8> {
    PREV_FIRE.get_or_init(|| Mutex::new(0))
}

pub fn init_uart(_stdout_path: Option<&Path>) -> io::Result<()> {
    // Initialize UART (already done via get_uart())
    let _ = get_uart();
    Ok(())
}

pub fn mmio_tick(top: *mut VerilatorTop) -> bool {
    unsafe {
        let cur_fire = crate::ffi::verilator_top_get_mmio_fire(top);
        let mut prev = get_prev_fire().lock().unwrap();
        let rising = *prev == 0 && cur_fire != 0;
        *prev = cur_fire;

        if !rising {
            return false;
        }

        let addr = crate::ffi::verilator_top_get_mmio_fire_addr(top);
        let data = crate::ffi::verilator_top_get_mmio_fire_data(top);

        if addr == SIM_EXIT_ADDR {
            let code = (data & 0xFFFFFFFF) as i32;
            if code == 0 {
                eprintln!("[MMIO] simulation success");
            } else {
                eprintln!("[MMIO] simulation exit code {}", code);
            }
            return true; // Signal exit
        } else if addr >= UART_BASE_ADDR && addr < UART_BASE_ADDR + 8 {
            // UART register access (16550 compatible)
            let offset = addr - UART_BASE_ADDR;
            let mut uart = get_uart().lock().unwrap();
            uart.mmio_store(offset, 1, data);
        }

        false
    }
}
