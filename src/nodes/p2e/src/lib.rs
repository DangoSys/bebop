pub mod builder;
pub mod ctb;
pub mod runner;
pub mod sim;

pub use builder::BitstreamBuilder;
pub use ctb::ffi;
pub use runner::{FlashBitstreamStep, InitStep, RunWorkloadStep, SimulationResult};
pub use sim::{run, P2ECli};

pub type Result<T> = std::result::Result<T, String>;
