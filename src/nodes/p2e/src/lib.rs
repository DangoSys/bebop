pub mod builder;
pub mod cli;
pub mod config;
pub mod ffi;
pub mod mmio;
pub mod runner;

pub use builder::BitstreamBuilder;
pub use cli::{run, P2ECli};
pub use config::{parse_args, BitstreamConfig, CliArgs};
pub use mmio::ScuController;
pub use runner::{
    FlashBitstreamStep, InitStep, P2ESimulator, RunWorkloadStep, SimulationResult, SimulatorConfig, VdbgSession,
};

pub type Result<T> = std::result::Result<T, String>;
