//! Default `bebop-chip-<chip>` entry: boot the complete PB-described chip.
use bebop_bemu::root::run::{Args, run};
use clap::Parser;

fn main() {
    if let Err(error) = run(Args::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
