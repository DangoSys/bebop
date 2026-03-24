/// BEMU 库入口
///
/// 这个库提供 Buckyball NPU 模拟功能
pub mod emu;
pub mod node;
pub mod shm;

pub use emu::bemu::Bemu;
