//! CLI: clap definitions and command dispatch.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};

use crate::graph::sim;
use crate::node::emu;

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
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,

    #[arg(long, global = true, default_value_t = false)]
    pub no_ipc_stats: bool,

    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[arg(long, hide = true, global = true)]
    pub node_file: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    Bemu {
        elf: PathBuf,
        #[arg(long, default_value_t = false)]
        step: bool,
        #[arg(long, default_value_t = false)]
        all_banks: bool,
    },

    #[cfg(feature = "verilator")]
    Verilator {
        elf: PathBuf,
        #[arg(long, default_value_t = false)]
        step: bool,
        #[arg(long, default_value_t = false)]
        all_banks: bool,
    },

    #[cfg(feature = "verilator")]
    Difftest {
        elf: PathBuf,
        #[arg(long, default_value_t = false)]
        step: bool,
        #[arg(long, default_value_t = false)]
        all_banks: bool,
    },

    #[command(hide = true, name = "bemu-tests")]
    BemuTests {
        #[arg(long, hide = true, default_value_t = false)]
        step: bool,
        #[arg(long, hide = true, default_value_t = false)]
        diff_all_banks: bool,
        #[arg(long, hide = true, value_name = "SHM")]
        shm_name: String,
    },

    #[cfg(all(feature = "verilator", unix))]
    #[command(hide = true, name = "verilator-engine")]
    VerilatorEngine {
        #[arg(long, hide = true, default_value_t = false)]
        step: bool,
        #[arg(long, hide = true, default_value_t = false)]
        diff_all_banks: bool,
        #[arg(long, hide = true, value_name = "SHM")]
        shm_name: String,
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
            sim::bemu(elf, step, all_banks, bemu_cfg, ipc_stats)
        }),
        #[cfg(feature = "verilator")]
        Commands::Verilator {
            elf,
            step,
            all_banks,
        } => run_timed("verilator", || {
            sim::verilator(elf, step, all_banks, bemu_cfg, ipc_stats)
        }),
        #[cfg(feature = "verilator")]
        Commands::Difftest {
            elf,
            step,
            all_banks,
        } => run_timed("difftest", || {
            sim::difftest(elf, step, all_banks, bemu_cfg, ipc_stats)
        }),
        Commands::BemuTests {
            step,
            diff_all_banks,
            shm_name,
        } => emu::bemu_tests(step, diff_all_banks, bemu_cfg, shm_name, ipc_stats),
        #[cfg(all(feature = "verilator", unix))]
        Commands::VerilatorEngine {
            step,
            diff_all_banks,
            shm_name,
        } => emu::vl_engine::run(step, diff_all_banks, shm_name, ipc_stats),
    }
}
