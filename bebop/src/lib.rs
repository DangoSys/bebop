pub mod buckyball;
pub mod simulator;

// 重新导出常用类型
pub use simulator::sim::mode::{SimConfig, SimMode};
pub use simulator::utils::{log, log_config};
