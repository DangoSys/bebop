pub mod builder;
pub mod config;
pub mod ffi;
pub mod mmio;
mod runner;
pub mod simulator;
pub mod vdbg;

pub use builder::BitstreamBuilder;
pub use mmio::ScuController;
pub use runner::{run, P2ECli};
pub use simulator::{P2ESimulator, SimulationResult, SimulatorConfig};
pub use vdbg::VdbgSession;

pub type Result<T> = std::result::Result<T, String>;
