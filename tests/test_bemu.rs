use clap::Parser;
use std::process::ExitCode;

#[path = "common/runner/bemu.rs"]
mod backend;
mod common;

use backend::BemuBackend;
use common::{run_elf_regression, RegressionArgs};

fn main() -> ExitCode {
    let args = RegressionArgs::parse();
    run_elf_regression(
        args,
        "test_bemu",
        |tc| format!("bemu::{}", tc.name),
        "Make sure to build the generated BEMU crate",
        BemuBackend,
    )
}
