mod args;
pub mod artifacts;
pub mod discovery;
pub mod runner;

pub use args::RegressionArgs;
pub use runner::run_elf_regression;
#[cfg(all(feature = "bemu", feature = "verilator"))]
pub use runner::BankHashDifftestBackend;
#[cfg(feature = "bemu")]
#[allow(unused_imports)]
pub use runner::BemuBackend;
#[cfg(feature = "p2e")]
pub use runner::P2eBackend;
#[cfg(feature = "verilator")]
#[allow(unused_imports)]
pub use runner::VerilatorBackend;
