//! Verilator cosim shim (objects from `build.rs` when feature `verilator` is on).

#![allow(clippy::duplicated_attributes)]

#[cfg(all(feature = "verilator", unix))]
mod dpi_mem;

#[cfg(all(feature = "verilator", unix))]
pub use dpi_mem::{
    set_mem16_reader as cosim_set_mem16_reader, set_mem16_writer as cosim_set_mem16_writer,
};

#[cfg(all(feature = "verilator", unix))]
#[link(name = "Vbebop_accel", kind = "static")]
#[link(name = "verilated", kind = "static")]
#[link(name = "stdc++", kind = "dylib")]
#[link(name = "atomic", kind = "dylib")]
unsafe extern "C" {
    fn bebop_cosim_init();
    fn bebop_cosim_set_digest_all_banks(v: u32);
    fn bebop_cosim_issue(funct: u32, xs1: u64, xs2: u64);
    fn bebop_cosim_read_result() -> u64;
    fn bebop_cosim_read_bank_digest_peek() -> u64;
    fn bebop_cosim_shutdown();
}

pub struct CosimGuard;

impl CosimGuard {
    pub fn new() -> Self {
        unsafe {
            bebop_cosim_init();
        }
        Self
    }
}

pub fn cosim_set_digest_all_banks(all: bool) {
    unsafe {
        bebop_cosim_set_digest_all_banks(if all { 1 } else { 0 });
    }
}

impl Drop for CosimGuard {
    fn drop(&mut self) {
        unsafe {
            bebop_cosim_shutdown();
        }
    }
}

pub fn cosim_issue(funct: u32, xs1: u64, xs2: u64) {
    unsafe {
        bebop_cosim_issue(funct, xs1, xs2);
    }
}

pub fn cosim_result() -> u64 {
    unsafe { bebop_cosim_read_result() }
}

/// RTL-only bank digest (0 = not implemented). Future: compare with BEMU `bank_hash` on same step.
pub fn cosim_bank_digest_peek() -> u64 {
    unsafe { bebop_cosim_read_bank_digest_peek() }
}
