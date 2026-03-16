/// BEMU 库入口
/// 
/// 这个库提供 Buckyball NPU 模拟功能
/// 可以被 Spike (C++) 通过 FFI 调用

pub mod emu;

// 重新导出常用类型
pub use emu::bemu::Bemu;
pub use emu::interface::spike_interface::BemuSpikeInterface;
