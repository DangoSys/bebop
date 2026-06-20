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
//===----------------------------------------------------------------------===//
//
// BEMU (Buckyball Emulator) wraps Spike ISA simulator with custom RoCC
// instructions for Buckyball accelerator emulation.
//
//===----------------------------------------------------------------------===//

use snafu::{OptionExt, ResultExt, Whatever};
use std::path::PathBuf;

use crate::spike::{run_spike_config, SpikeRunConfig};
use crate::trace::{init_trace, shutdown_trace, TraceConfig};

#[derive(Debug, Clone)]
pub struct BemuCli {
    pub elf: PathBuf,
    pub log_dir: Option<PathBuf>,
    pub pk: bool,
    pub itrace: bool,
    pub mtrace: bool,
    pub banktrace: bool,
}

pub fn run(cli: BemuCli) -> Result<(), Whatever> {
    let elf_file_str = cli.elf.to_str().whatever_context("invalid elf path")?;
    let log_dir = cli.log_dir.as_ref().whatever_context(
        "--log-dir is required: bemu enables Spike debug mode and must write disasm.log; \
         pass --log-dir=<dir> (e.g. --log-dir=/tmp/bemu_log)",
    )?;
    std::fs::create_dir_all(log_dir).ok();
    let disasm_log_file = log_dir.join("disasm.log");
    let disasm_log_file_str: &str = disasm_log_file.to_str().whatever_context("invalid log_dir path")?;

    //===------------ Initialize BEMU trace logging ----------------------------===//
    let trace_config = TraceConfig::new(cli.itrace, cli.mtrace, cli.banktrace);
    init_trace(&log_dir, trace_config).whatever_context("failed to init bemu trace")?;

    //===------------ Run BEMU simulation -------------------------------------===//
    let spike_config = SpikeRunConfig::new(elf_file_str, disasm_log_file_str, cli.pk);
    let result = run_spike_config(spike_config).whatever_context("spike execution failed");
    if let Err(e) = shutdown_trace() {
        eprintln!("Warning: failed to flush BEMU trace: {e}");
    }
    result
}
