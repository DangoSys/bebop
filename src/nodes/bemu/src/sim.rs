//===- sim.rs - BEMU simulation entry point -------------------------------===//
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
//===-----------------------------------------------------------------===//-----===//
//
// BEMU (Buckyball Emulator) wraps Spike ISA simulator with custom RoCC
// instructions for Buckyball accelerator emulation.
//
//===-----------------------------------------------------------------===//-----===//

use snafu::{FromString, OptionExt, ResultExt, Whatever};
use std::path::PathBuf;

use crate::ffi::run_spike;
use crate::trace::{init_bank_hash_trace, init_trace, shutdown_bank_hash_trace, TraceConfig};

// Default configuration
const DEFAULT_ISA: &str = "rv64gc";
const DEFAULT_PROCS: usize = 1;
const DEFAULT_MEM_MB: usize = 2048;

#[derive(Debug, Clone)]
pub struct BemuCli {
    pub elf: PathBuf,
    pub log_dir: Option<PathBuf>,
    pub pk: bool,
    pub itrace: bool,
    pub mtrace: bool,
    pub banktrace: bool,
    pub bank_hash_stream: Option<PathBuf>,
}

pub fn run(cli: BemuCli) -> Result<(), Whatever> {
    let elf_path = cli.elf.to_str().whatever_context("invalid elf path")?;
    let log_dir = cli.log_dir.as_ref().whatever_context(
        "--log-dir is required: bemu enables Spike debug mode and must write disasm.log; \
         pass --log-dir=<dir> (e.g. --log-dir=/tmp/bemu_log)",
    )?;
    std::fs::create_dir_all(log_dir).ok();
    let log_file_path = log_dir.join("disasm.log");
    let log_path = log_file_path.to_str().whatever_context("invalid log_dir path")?;

    // Initialize BEMU trace logging
    let trace_path = log_dir.join("bdb.ndjson");
    init_trace(
        &trace_path,
        TraceConfig {
            itrace: cli.itrace,
            mtrace: cli.mtrace,
            banktrace: cli.banktrace,
        },
    )
    .map_err(|e| Whatever::without_source(format!("Failed to init bemu trace: {}", e)))?;

    let bank_hash_trace_path = log_dir.join("bemu_bank_hash.ndjson");
    init_bank_hash_trace(&bank_hash_trace_path, cli.bank_hash_stream.as_deref())
        .map_err(|e| Whatever::without_source(format!("Failed to init bemu bank hash trace: {}", e)))?;
    println!("BEMU bank hash trace: {}", bank_hash_trace_path.display());
    println!(
        "BEMU canonical bank hash trace: {}",
        bank_hash_trace_path
            .with_file_name("bemu_bank_hash.canonical.ndjson")
            .display()
    );

    if let Some(path) = &cli.bank_hash_stream {
        println!("BEMU runtime bank hash packet stream: {}", path.display());
    }

    let result = run_spike(
        DEFAULT_ISA,
        DEFAULT_PROCS,
        DEFAULT_MEM_MB,
        elf_path,
        Some(log_path),
        cli.pk,
    )
    .whatever_context("spike execution failed");
    if let Err(e) = shutdown_bank_hash_trace() {
        eprintln!("Warning: failed to flush BEMU bank hash packet stream: {e}");
    }
    result
}
