pub mod ffi;
pub mod ddr;
pub mod scu;
pub mod simulator;

pub use simulator::P2ESimulator;
pub use ddr::DdrBackdoor;
pub use scu::ScuController;

pub type Result<T> = std::result::Result<T, String>;
