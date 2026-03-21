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
    /// `cmake` + `ninja` → `src/workload/build/libbebop_rocc.so`（需本机有 `spike`、`cmake`、`ninja`）。
    Build,
    /// Run Spike + pk：`elf` 为测例路径；RoCC 库为 `src/workload/build/libbebop_rocc.so`（先 `bebop build`）。
    SpikeTest { elf: PathBuf },
    #[command(hide = true, name = "worker-shm")]
    WorkerShm { name: String },
}

pub fn dispatch(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::ShmSmoke { size } => shm::run_smoke(size),
        Commands::Build => bebop::build_workload(),
        Commands::SpikeTest { elf } => bebop::spike_tests(elf),
        Commands::WorkerShm { name } => bebop::worker_shm(name),
    }
}
