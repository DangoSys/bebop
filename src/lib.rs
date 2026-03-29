pub mod emu;
pub mod node;
pub mod shm;
/// BEMU 库入口
///
/// 这个库提供 Buckyball NPU 模拟功能
#[cfg(feature = "verilator")]
mod verilator;

pub use emu::bemu::Bemu;
