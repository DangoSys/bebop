//! CLI：clap 定义与命令分发（Spike 仿真在 [`crate::spike::runner`]）。

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::emu;
use crate::spike;

#[derive(Parser)]
#[command(name = "bebop", about = "Bebop BEMU CLI")]
pub struct Cli {
    /// Enable INFO logs (and for spike-test, the BEMU worker child inherits via RUST_LOG).
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, hide = true, global = true)]
    pub node_file: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run Spike in spike mode. Run `bebop spike-test -h` to see more details.
    SpikeTest {
        elf: PathBuf,
        /// After each RoCC custom instruction: print bank state hash (64-bit per bank).
        #[arg(long, default_value_t = false)]
        step: bool,
    },

    /// Spike + BEMU golden model + Verilator cosim (same shm protocol as spike-test).
    #[cfg(feature = "verilator")]
    Verilator {
        elf: PathBuf,
        #[arg(long, default_value_t = false)]
        step: bool,
    },

    //===----------------------------------------------------------------------===//
    //
    // The functions below are not exposed to the CLI.
    // They are used internally by the CLI.
    //
    //===----------------------------------------------------------------------===//
    #[command(hide = true, name = "bemu-tests")]
    BemuTests {
        #[arg(long, hide = true, default_value_t = false)]
        step: bool,
        #[arg(long, hide = true, default_value_t = false)]
        diff_all_banks: bool,
    },

    #[cfg(feature = "verilator")]
    #[command(hide = true, name = "verilator-worker")]
    VerilatorWorker {
        #[arg(long, hide = true, default_value_t = false)]
        step: bool,
        #[arg(long, hide = true, default_value_t = false)]
        diff_all_banks: bool,
    },
}

pub fn dispatch(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::SpikeTest { elf, step } => spike::runner::spike_tests(elf, step),
        #[cfg(feature = "verilator")]
        Commands::Verilator { elf, step } => spike::runner::verilator_tests(elf, step),
        Commands::BemuTests {
            step,
            diff_all_banks,
        } => emu::bemu_tests(step, diff_all_banks),
        #[cfg(feature = "verilator")]
        Commands::VerilatorWorker {
            step,
            diff_all_banks,
        } => emu::vl_worker::vl_worker_tests(step, diff_all_banks),
    }
}
