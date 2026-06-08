// SCU (System Control Unit) interface for Verilator simulation
// SCU DPI-C functions (scu_uart_write, scu_sim_exit) are called automatically
// from RTL when software writes to SCU registers. This module provides helpers
// to query the SCU state from Rust.

use std::io;
use std::path::Path;

pub fn init_uart(_stdout_path: Option<&Path>) -> io::Result<()> {
    Ok(())
}

/// Check if simulation should exit (called from RTL via scu_sim_exit DPI-C)
pub fn should_exit() -> bool {
    // SAFETY: FFI call to C++ Verilator runtime; function is extern "C" and always safe to call.
    unsafe { crate::ffi::verilator_scu_has_exit() }
}

pub fn push_uart_rx(hart_id: u32, byte: u8) {
    // SAFETY: FFI call to C++ Verilator runtime; function appends one byte to
    // the protected SCU RX queue.
    unsafe { crate::ffi::verilator_scu_push_uart_rx(hart_id, byte as u32) }
}

pub fn drain_uart_tx(buf: &mut [u32]) -> usize {
    // SAFETY: FFI call to C++ Verilator runtime; `buf` is valid for `len`
    // elements and the function writes at most that many u32 values.
    unsafe { crate::ffi::verilator_scu_drain_uart_tx(buf.as_mut_ptr(), buf.len() as u32) as usize }
}
