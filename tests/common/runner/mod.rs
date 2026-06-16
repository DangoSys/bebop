mod backend;
mod exec;
mod regression;

#[cfg(all(feature = "bemu", feature = "verilator"))]
pub use backend::BankHashDifftestBackend;
#[cfg(feature = "bemu")]
pub use backend::BemuBackend;
#[cfg(feature = "p2e")]
pub use backend::P2eBackend;
#[cfg(feature = "verilator")]
pub use backend::VerilatorBackend;
pub use regression::run_elf_regression;
