mod bebop;
mod emu;
mod shm;

use clap::Parser;
use log::error;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = bebop::Cli::parse();
    if cli.verbose {
        log::set_max_level(log::LevelFilter::Debug);
    }
    if let Err(e) = bebop::dispatch(cli) {
        error!("{e}");
        std::process::exit(1);
    }
}
