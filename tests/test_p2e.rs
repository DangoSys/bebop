use clap::Parser;
use std::process::ExitCode;

#[path = "common/runner/p2e.rs"]
mod backend;
mod common;

use backend::P2eBackend;
use common::{run_elf_regression, RegressionArgs};

fn main() -> ExitCode {
    let args = RegressionArgs::parse();
    let bitstream = args.p2e_bitstream();
    let diff = args.diff;
    run_elf_regression(
        args,
        "test_p2e",
        |tc| format!("p2e::{}", tc.name),
        "Make sure to build with: cargo build --features p2e",
        P2eBackend::new(bitstream, diff),
    )
}
