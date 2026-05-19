//! BEMU ELF Regression Tests
//!
//! This integration test discovers ELF files and runs them using the bebop bemu backend.
//!
//! Usage:
//!   cargo test --test test_bemu --features bemu -- --help
//!   cargo test --test test_bemu --features bemu
//!   cargo test --test test_bemu --features bemu -- --filter matmul
//!   cargo nextest run --test test_bemu --features bemu
//!
//! Note: This test requires the 'bemu' feature to be enabled.

use clap::Parser;
use std::process::ExitCode;

mod common;

use common::{run_elf_regression, BemuBackend, RegressionArgs};

fn main() -> ExitCode {
    let args = RegressionArgs::parse();
    run_elf_regression(
        args,
        "test_bemu",
        |tc| format!("bemu::{}", tc.name),
        "Make sure to build with: cargo build --features bemu",
        BemuBackend,
    )
}
