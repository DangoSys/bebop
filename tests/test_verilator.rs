use clap::Parser;
use std::process::ExitCode;

mod common;

use common::{run_elf_regression, RegressionArgs, VerilatorBackend};

fn main() -> ExitCode {
  let args = RegressionArgs::parse();
  run_elf_regression(
    args,
    "test_verilator",
    |tc| format!("verilator::{}", tc.name),
    "Make sure to build with: cargo build --features verilator",
    VerilatorBackend,
  )
}
