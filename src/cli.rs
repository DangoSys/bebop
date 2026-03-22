//! CLI：clap 定义与命令分发（仿真在 [`crate::bebop`]）。

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::{bebop, shm};

#[derive(Parser)]
#[command(name = "bebop", about = "Bebop BEMU CLI")]
pub struct Cli {
    /// Enable INFO logs (and for spike-test, the BEMU worker child inherits via RUST_LOG).
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(hide = true)]
    ShmSmoke {
        #[arg(long, default_value_t = 4096)]
        size: usize,
    },
    Build,
    /// Run Spike + pk：`elf` is the path to the ELF file.
    SpikeTest {
        elf: PathBuf,
        /// After each RoCC custom instruction: print bank state hash (64-bit per bank).
        #[arg(long, default_value_t = false)]
        step: bool,
    },
    #[command(hide = true, name = "worker-shm")]
    WorkerShm {
        name: String,
    },
}

pub fn dispatch(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::ShmSmoke { size } => shm::run_smoke(size),
        Commands::Build => bebop::build_workload(),
        Commands::SpikeTest { elf, step } => bebop::spike_tests(elf, step),
        Commands::WorkerShm { name } => bebop::worker_shm(name),
    }
}
