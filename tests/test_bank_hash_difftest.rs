//! Bank Hash Difftest ELF Regression Tests
//!
//! This integration test discovers ELF files and runs them through the one-command
//! BEMU/Verilator bank hash difftest flow.
//!
//! Usage:
//!   cargo test --test test_bank_hash_difftest --features "bemu,verilator" -- --help
//!   cargo test --test test_bank_hash_difftest --features "bemu,verilator"
//!   cargo test --test test_bank_hash_difftest --features "bemu,verilator" -- --filter matmul
//!   cargo nextest run --test test_bank_hash_difftest --features "bemu,verilator"
//!
//! Note: This test requires both 'bemu' and 'verilator' features to be enabled.
//!       It is resource-intensive because each test runs both backends.

use clap::Parser;
use std::process::ExitCode;

mod common;

use common::{run_elf_regression, BankHashDifftestBackend, RegressionArgs};

fn main() -> ExitCode {
    let args = RegressionArgs::parse();
    run_elf_regression(
        args,
        "test_bank_hash_difftest",
        |tc| format!("bank_hash_difftest::{}", tc.name),
        "Make sure to build with: cargo build --features \"bemu,verilator\"",
        BankHashDifftestBackend,
    )
}
