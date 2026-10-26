pub mod barrier;
pub mod context;
pub mod decode;
mod dispatch;

pub use decode::{FUNCT7_MSET, FUNCT7_MVIN_MMIO};
pub use dispatch::{cycles_after_issue, execute_known};
pub mod kernel;
pub mod mset;
pub mod mvin;
pub mod mvin_2d;
pub mod mvin_mmio;
pub mod mvout;
