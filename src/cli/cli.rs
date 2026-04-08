//! CLI: clap definitions and command dispatch (Spike simulation lives in [`crate::spike::runner`]).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};

use crate::emu;
use crate::spike;

fn fmt_elapsed(d: Duration) -> String {
    let ms = d.as_millis();
    if ms >= 60_000 {
        let m = ms / 60_000;
        let s = (ms % 60_000) / 1000;
        format!("{m}m {s:02}s")
    } else {
        format!("{:.2}s", d.as_secs_f64())
    }
}

fn run_timed(label: &str, run: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    let t0 = Instant::now();
    let out = run();
    if out.is_ok() {
        println!("    Finished `{label}` in {}", fmt_elapsed(t0.elapsed()));
    }
    out
}

#[derive(Parser)]
#[command(name = "bebop", about = "Bebop BEMU CLI")]
pub struct Cli {
    /// Enable INFO logs (Spike/worker children may inherit via RUST_LOG).
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,

    /// Do not print SHM IPC timing summary (default is on for `bemu` / `verilator` / `difftest`).
    #[arg(long, global = true, default_value_t = false)]
    pub no_ipc_stats: bool,

    #[command(subcommand)]
    pub command: Commands,

    /// BEMU `config.toml`; forwarded to `bemu-tests` workers.
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

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
        /// Print all banks in step mode (default: allocated banks only).
        #[arg(long, default_value_t = false)]
        all_banks: bool,
    },

    /// Spike + dual SHM lanes: `bemu-tests` + `verilator-engine` in parallel per RoCC; `rd` must match.
    #[cfg(feature = "verilator")]
    Verilator {
        elf: PathBuf,
        #[arg(long, default_value_t = false)]
        step: bool,
        #[arg(long, default_value_t = false)]
        all_banks: bool,
    },

    /// Like `verilator`, plus Spike enforces **bank_digest** (FNV) match between lanes.
    #[cfg(feature = "verilator")]
    Difftest {
        elf: PathBuf,
        #[arg(long, default_value_t = false)]
        step: bool,
        #[arg(long, default_value_t = false)]
        all_banks: bool,
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
    let bemu_cfg = cli.config.clone();
    let ipc_stats = !cli.no_ipc_stats;
    match cli.command {
        Commands::Bemu {
            elf,
            step,
            all_banks,
        } => run_timed("bemu", || {
            spike::runner::spike_tests(elf, step, all_banks, bemu_cfg, ipc_stats)
        }),
        #[cfg(feature = "verilator")]
        Commands::Verilator {
            elf,
            step,
            all_banks,
        } => run_timed("verilator", || {
            spike::runner::verilator_tests(elf, step, all_banks, bemu_cfg, ipc_stats)
        }),
        #[cfg(feature = "verilator")]
        Commands::Difftest {
            elf,
            step,
            all_banks,
        } => run_timed("difftest", || {
            spike::runner::difftest(elf, step, all_banks, bemu_cfg, ipc_stats)
        }),
        Commands::BemuTests {
            step,
            diff_all_banks,
        } => emu::bemu_tests(step, diff_all_banks, bemu_cfg),
        #[cfg(all(feature = "verilator", unix))]
        Commands::VerilatorEngine {
            step,
            diff_all_banks,
        } => emu::vl_engine::run(step, diff_all_banks),
    }
}
