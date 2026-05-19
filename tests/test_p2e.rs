//! P2E HEX Regression Tests
//!
//! This integration test discovers HEX files and runs them using the bebop p2e backend.
//!
//! Usage:
//!   cargo test --test test_p2e --features p2e -- --help
//!   BEBOP_P2E_BITSTREAM=<BIT> BEBOP_P2E_BUILD_DIR=<DIR> cargo nextest run --test test_p2e --features p2e
//!
//! Required environment variables:
//!   BEBOP_P2E_BITSTREAM - path to the FPGA bitstream
//!   BEBOP_P2E_BUILD_DIR - path to the build directory
//!
//! Note: This test requires the 'p2e' feature to be enabled.

use clap::Parser;
use std::process::ExitCode;

mod common;

use common::{run_elf_regression, P2eBackend, RegressionArgs};

fn main() -> ExitCode {
    let args = RegressionArgs::parse();
    let bitstream = args
        .p2e_bitstream()
        .expect("BEBOP_P2E_BITSTREAM env var is required for test_p2e");
    let build_dir = args
        .p2e_build_dir()
        .expect("BEBOP_P2E_BUILD_DIR env var is required for test_p2e");
    run_elf_regression(
        args,
        "test_p2e",
        |tc| format!("p2e::{}", tc.name),
        "Make sure to build with: cargo build --features p2e",
        P2eBackend::new(bitstream, build_dir),
    )
}
