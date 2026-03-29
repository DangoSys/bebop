//! Verilator cosim shim (objects from `build.rs` when feature `verilator` is on).

#![allow(clippy::duplicated_attributes)]

#[cfg(all(feature = "verilator", unix))]
#[link(name = "Vbebop_accel", kind = "static")]
#[link(name = "verilated", kind = "static")]
#[link(name = "stdc++", kind = "dylib")]
#[link(name = "atomic", kind = "dylib")]
unsafe extern "C" {
    fn bebop_cosim_init();
    fn bebop_cosim_issue(funct: u32, xs1: u64, xs2: u64);
    fn bebop_cosim_read_result() -> u64;
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
