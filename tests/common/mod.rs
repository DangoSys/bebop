mod args;
pub mod artifacts;
pub mod discovery;
pub mod runner;

pub use args::RegressionArgs;
pub use runner::run_elf_regression;
#[cfg(feature = "bemu")]
pub use runner::BemuBackend;
#[cfg(feature = "p2e")]
pub use runner::P2eBackend;
#[cfg(feature = "verilator")]
pub use runner::VerilatorBackend;
