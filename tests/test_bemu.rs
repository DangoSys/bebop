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
        "Make sure to build with an explicit chip feature, for example: cargo build --features bemu-toy",
        BemuBackend,
    )
}
