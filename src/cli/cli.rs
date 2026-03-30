//! CLI：clap 定义与命令分发（Spike 仿真在 [`crate::spike::runner`]）。

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::emu;
use crate::spike;

#[derive(Parser)]
#[command(name = "bebop", about = "Bebop BEMU CLI")]
pub struct Cli {
    /// Enable INFO logs (Spike/worker children may inherit via RUST_LOG).
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, hide = true, global = true)]
    pub node_file: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Spike + pk + BEMU RoCC sidecar (golden model).
    Bemu {
        elf: PathBuf,
        /// After each RoCC custom instruction: print bank state hash (64-bit per bank).
        #[arg(long, default_value_t = false)]
        step: bool,
    },

    /// Spike + dual SHM lanes: `bemu-tests` + `verilator-engine` in parallel per RoCC; `rd` must match.
    #[cfg(feature = "verilator")]
    Verilator {
        elf: PathBuf,
        #[arg(long, default_value_t = false)]
        step: bool,
    },

    /// Like `verilator`, plus Spike enforces **bank_digest** (FNV) match between lanes.
    #[cfg(feature = "verilator")]
    Difftest {
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

    #[cfg(all(feature = "verilator", unix))]
    #[command(hide = true, name = "verilator-engine")]
    VerilatorEngine {
        #[arg(long, hide = true, default_value_t = false)]
        step: bool,
        #[arg(long, hide = true, default_value_t = false)]
        diff_all_banks: bool,
    },
}

pub fn dispatch(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::Bemu { elf, step } => spike::runner::spike_tests(elf, step),
        #[cfg(feature = "verilator")]
        Commands::Verilator { elf, step } => spike::runner::verilator_tests(elf, step),
        #[cfg(feature = "verilator")]
        Commands::Difftest { elf, step } => spike::runner::difftest(elf, step),
        Commands::BemuTests {
            step,
            diff_all_banks,
        } => emu::bemu_tests(step, diff_all_banks),
        #[cfg(all(feature = "verilator", unix))]
        Commands::VerilatorEngine {
            step,
            diff_all_banks,
        } => emu::vl_engine::run(step, diff_all_banks),
    }
}
