//! CLI：clap 定义与命令分发（仿真在 [`crate::bebop`]，workload 在 [`crate::workload`]）。

use clap::{Parser, Subcommand};

use crate::{bebop, shm, workload};

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
    /// `cmake` + `ninja`（`src/workload`：RISC-V ELF + `libbebop_rocc.so`）。
    Workload,
    /// Run Spike + pk（需已 `cargo build --release` 且 `bebop workload`）。
    SpikeTest {
        #[arg(long, default_value_t = false)]
        all: bool,
    },
    #[command(hide = true, name = "worker-shm")]
    WorkerShm { name: String },
}

pub fn dispatch(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::ShmSmoke { size } => shm::run_smoke(size),
        Commands::Workload => workload::cmake_ninja(),
        Commands::SpikeTest { all } => bebop::spike_tests(all),
        Commands::WorkerShm { name } => bebop::worker_shm(name),
    }
}
