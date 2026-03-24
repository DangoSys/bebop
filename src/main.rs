mod cli;
mod emu;
mod shm;
mod spike;
mod utils;

use crate::cli::cli::{dispatch, Cli};
use crate::utils::log::init_log;
use clap::Parser;

fn main() {
    //===----------------------------------------------------------------------===//
    //
    // All commands come through here to CLI, then start the execution.
    //
    //===----------------------------------------------------------------------===//
    let cli = Cli::parse();
    init_log(cli.verbose);

    if let Err(e) = dispatch(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
