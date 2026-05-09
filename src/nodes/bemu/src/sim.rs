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
// Flow: BemuCli -> BemuConfig -> run_spike (FFI)
//
//===----------------------------------------------------------------------===//

use snafu::{OptionExt, ResultExt, Whatever};
use std::path::PathBuf;

use crate::config::BemuConfig;
use crate::ffi::run_spike;

#[derive(Debug, Clone)]
pub struct BemuCli {
    pub elf: PathBuf,
    pub args: Vec<String>,
}

pub fn run(cli: BemuCli) -> Result<(), Whatever> {
    let config = BemuConfig::parse(cli.args)?;
    run_simulation(cli.elf, config)
}

fn run_simulation(elf: PathBuf, config: BemuConfig) -> Result<(), Whatever> {
    let elf_path = elf.to_str().whatever_context("invalid elf path")?;

    run_spike(
        &config.isa,
        config.procs,
        config.mem_mb,
        elf_path,
        config.log.as_deref(),
    )
    .whatever_context("spike execution failed")
}
