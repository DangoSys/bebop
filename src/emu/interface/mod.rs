//! BEMU 接口层
//! 
//! 提供 BEMU 与外部仿真器（如 Spike）的接口
//! 
//! # 模块结构
//! 
//! - [`decoder`]: 指令解码模块，负责解析和解码 Buckyball 自定义指令
//! - [`memory_map`]: 内存映射模块，管理物理地址到虚拟地址的转换
//! - [`spike_interface`]: Spike 回调接口，提供与 Spike 集成的标准化接口
//! 
//! # 使用示例
//! 
//! ```rust,no_run
//! use bebop::arch::bemu::interface::{
//!     BemuSpikeInterface, SpikeCallbacks, SpikeCallbackParams
//! };
//! 
//! // 创建接口实例
//! let mut interface = BemuSpikeInterface::with_verbose(true);
//! 
//! // 初始化内存映射
//! interface.init_memory_map().unwrap();
//! 
//! // 处理自定义指令
//! let params = SpikeCallbackParams::new(23, 0, 4 | (4 << 5) | (1 << 10));
//! match interface.handle_custom_instruction(&params) {
//!     Ok(result) => println!("Instruction result: 0x{:x}", result),
//!     Err(e) => eprintln!("Error: {:?}", e),
//! }
//! 
//! // 同步内存
//! let data = [0x11, 0x22, 0x33, 0x44];
//! interface.sync_memory(0x100, &data).unwrap();
//! 
//! // 获取统计信息
//! let stats = interface.get_stats();
//! println!("Instructions executed: {}", stats.instructions_executed);
//! ```

pub mod decoder;
pub mod memory_map;
pub mod spike_interface;

pub use decoder::{InstructionDecoder, DecodedInstruction, InstructionType};
pub use memory_map::{MemoryMapper, MemoryRegion, MemoryPermission, PAGE_SIZE};
pub use spike_interface::{
    BemuSpikeInterface, SpikeCallbacks, SpikeCallbackParams, SpikeError, SpikeResult,
};
