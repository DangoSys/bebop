pub mod emu;
pub mod node;
pub mod shm;
pub mod utils;
/// BEMU library entry point.
///
/// This crate provides Buckyball NPU emulation.
#[cfg(feature = "verilator")]
mod verilator;

pub use emu::bemu::Bemu;
