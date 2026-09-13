use clap::Parser;
use std::process::ExitCode;

mod common;

use common::{run_elf_regression, RegressionArgs, VerilatorBackend};

fn main() -> ExitCode {
    let args = RegressionArgs::parse();
    let diff = cfg!(feature = "difftest");
    let test_prefix = if diff { "difftest" } else { "verilator" };
    run_elf_regression(
        args,
        "test_verilator",
        move |tc| format!("{}::{}", test_prefix, tc.name),
        "Make sure to build with: cargo build --features verilator",
        VerilatorBackend::new(diff),
    )
}
