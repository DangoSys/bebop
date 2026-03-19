//! BEMU 接口层
//! 
//! 提供 BEMU 与外部仿真器（如 Spike）的接口
//! 
//! # 模块结构
//! 
//! - [`spike_interface`]: Spike 回调接口，提供与 Spike 集成的标准化接口
//! - [`capi_exports`]: C API 导出模块，提供 C 兼容接口供 Spike 调用

pub mod spike_interface;
pub mod capi_exports;

pub use spike_interface::{
    BemuSpikeInterface, SpikeCallbacks, SpikeCallbackParams
};
