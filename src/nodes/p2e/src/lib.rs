pub mod builder;
pub mod cli;
pub mod config;
pub mod ffi;
pub mod mmio;
pub mod runner;

pub use builder::BitstreamBuilder;
pub use cli::{run, P2ECli};
pub use config::{BitstreamConfig, CliArgs, parse_args};
pub use mmio::ScuController;
pub use runner::{
    P2ESimulator, SimulationResult, SimulatorConfig, VdbgSession,
    FlashBitstreamStep, InitStep, RunWorkloadStep,
};

pub type Result<T> = std::result::Result<T, String>;
