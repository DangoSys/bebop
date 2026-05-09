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

use crate::ffi::run_spike;

// Default configuration
const DEFAULT_ISA: &str = "rv64gc";
const DEFAULT_PROCS: usize = 1;
const DEFAULT_MEM_MB: usize = 2048;

#[derive(Debug, Clone)]
pub struct BemuCli {
    pub elf: PathBuf,
    pub log_dir: Option<PathBuf>,
}

pub fn run(cli: BemuCli) -> Result<(), Whatever> {
    let elf_path = cli.elf.to_str().whatever_context("invalid elf path")?;
    let log_path = cli.log_dir.as_ref().and_then(|d| d.to_str());

    run_spike(DEFAULT_ISA, DEFAULT_PROCS, DEFAULT_MEM_MB, elf_path, log_path).whatever_context("spike execution failed")
}
