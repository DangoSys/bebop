mod cli;
mod emu;
mod shm;
mod spike;
use std::env;

use clap::Parser;

fn main() {
    let cli = cli::Cli::parse();
    // `worker-shm` is a separate process: it never sees `-v` on argv. Put level in env so child inherits.
    if cli.verbose && env::var_os("RUST_LOG").is_none() {
        env::set_var("RUST_LOG", "info");
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("off")).init();

    if let Err(e) = cli::dispatch(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
