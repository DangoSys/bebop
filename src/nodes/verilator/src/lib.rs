mod sim;

#[path = "../native/ffi.rs"]
mod ffi;

#[path = "trace/dpi.rs"]
mod dpi;

#[path = "trace/trace.rs"]
mod trace;

#[path = "mmio/mmio.rs"]
mod mmio;

#[path = "main.rs"]
mod main_impl;

pub use main_impl::{run, VerilatorCli};
