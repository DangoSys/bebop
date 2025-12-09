pub mod simulator;
pub mod buckyball;

// 重新导出常用类型
pub use simulator::sim::mode::{SimMode, SimConfig};
pub use simulator::utils::{log, log_config};