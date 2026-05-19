// SCU (System Control Unit) interface for Verilator simulation
// SCU DPI-C functions (scu_uart_write, scu_sim_exit) are called automatically
// from RTL when software writes to SCU registers. This module provides helpers
// to query the SCU state from Rust.

use bebop_uart::Uart;
use std::io;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

static UART: OnceLock<Mutex<Uart>> = OnceLock::new();

fn get_uart() -> &'static Mutex<Uart> {
    UART.get_or_init(|| Mutex::new(Uart::new()))
}

pub fn init_uart(_stdout_path: Option<&Path>) -> io::Result<()> {
    get_uart();
    Ok(())
}

/// Check if simulation should exit (called from RTL via scu_sim_exit DPI-C)
pub fn should_exit() -> bool {
    // SAFETY: FFI call to C++ Verilator runtime; function is extern "C" and always safe to call.
    unsafe { crate::ffi::verilator_scu_has_exit() }
}

/// Get exit code (valid only if should_exit() returns true)
pub fn exit_code() -> i32 {
    // SAFETY: FFI call to C++ Verilator runtime; function is extern "C" and always safe to call.
    unsafe { crate::ffi::verilator_scu_exit_code() }
}

/// Reset SCU state (call at start of new simulation)
pub fn reset() {
    // SAFETY: FFI call to C++ Verilator runtime; function is extern "C" and always safe to call.
    unsafe { crate::ffi::verilator_scu_reset() }
}
