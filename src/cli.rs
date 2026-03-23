//! CLI：clap 定义与命令分发（仿真在 [`crate::bebop`]）。

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::emu::experiment::bank_rename;
use crate::shm;
use crate::spike::spike_runner;

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
    /// Run Spike + pk：`elf` is the path to the ELF file.
    SpikeTest {
        elf: PathBuf,
        /// After each RoCC custom instruction: print bank state hash (64-bit per bank).
        #[arg(long, default_value_t = false)]
        step: bool,
    },
    /// Parse `step=` lines from `spike-test --step` output; compare NoRename vs WriteAlias scoreboard.
    #[command(name = "bank-rename")]
    BankRename {
        #[arg(short, long)]
        log: PathBuf,
        #[arg(long, default_value_t = false)]
        bemu_latency: bool,
        #[arg(short, long, default_value = "1")]
        latency: Vec<u64>,
    },
    #[command(hide = true, name = "worker-shm")]
    WorkerShm { name: String },
}

pub fn dispatch(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::ShmSmoke { size } => shm::run_smoke(size),
        Commands::SpikeTest { elf, step } => spike_runner::spike_tests(elf, step),
        Commands::BankRename {
            log,
            bemu_latency,
            latency,
        } => bank_rename::run_rocc_step(log, bemu_latency, latency),
        Commands::WorkerShm { name } => spike_runner::worker_shm(name),
    }
}
