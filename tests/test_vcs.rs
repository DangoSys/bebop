use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

#[path = "common/runner/vcs.rs"]
mod backend;
mod common;

#[derive(Parser)]
struct Args {
    #[command(flatten)]
    regression: common::RegressionArgs,
    #[arg(long)]
    simv: PathBuf,
    #[arg(long)]
    dram_configs: PathBuf,
    #[arg(long)]
    log_dir: PathBuf,
}

fn main() -> ExitCode {
    let args = Args::parse();
    common::artifacts::ArtifactManager::set_root(args.log_dir);
    common::run_elf_regression(
        args.regression,
        "test_vcs",
        |case| format!("vcs::{}", case.name),
        "Build the selected simulation entry's bebop-vcs runner",
        backend::VcsBackend {
            simv: args.simv,
            dram_configs: args.dram_configs,
        },
    )
}
