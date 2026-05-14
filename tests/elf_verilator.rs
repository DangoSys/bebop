//! Verilator ELF Regression Tests
//!
//! This integration test discovers ELF files and runs them using the bebop verilator backend.
//!
//! Usage:
//!   cargo test --test elf_verilator --features verilator -- --help
//!   cargo test --test elf_verilator --features verilator
//!   cargo test --test elf_verilator --features verilator -- --filter matmul
//!   cargo nextest run --test elf_verilator --features verilator
//!
//! Note: This test requires the 'verilator' feature to be enabled.
//!       Verilator tests are resource-intensive and should be run with limited concurrency.

use clap::Parser;
use std::process::ExitCode;

mod common;

use common::{run_elf_regression, RegressionArgs, VerilatorBackend};

fn main() -> ExitCode {
    let args = RegressionArgs::parse();
    run_elf_regression(
        args,
        "elf_verilator",
        |tc| format!("verilator::{}", tc.name),
        "Make sure to build with: cargo build --features verilator",
        VerilatorBackend,
    )
}
