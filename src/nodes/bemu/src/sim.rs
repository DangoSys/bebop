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
use std::path::Path;

use crate::{spike::SpikeInstance, trace::TraceConfig};

pub struct BemuInstance {
    spike: SpikeInstance,
}

impl BemuInstance {
    pub fn new(log_dir: &Path, trace_config: TraceConfig) -> Result<Self, Whatever> {
        Ok(Self {
            spike: SpikeInstance::new(log_dir, trace_config).whatever_context("failed to create spike instance")?,
        })
    }

    pub fn load_elf(&mut self, elf: &Path) -> Result<(), Whatever> {
        let elf = elf.to_str().whatever_context("invalid elf path")?;
        self.spike.load_elf(elf).whatever_context("failed to load bemu elf")
    }

    pub fn init_hart(&mut self, pk: bool) -> Result<(), Whatever> {
        self.spike
            .init_hart(pk)
            .whatever_context("failed to initialize bemu hart")
    }

    pub fn step(&mut self) -> Result<(), Whatever> {
        self.spike.step().whatever_context("bemu step failed")
    }

    pub fn finished(&self) -> bool {
        self.spike.finished()
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.spike.exit_code()
    }

    pub fn total_latency(&self) -> u64 {
        self.spike.total_latency()
    }

    /// Return an owned snapshot of every physical private Bank.
    pub fn scratchpad_snapshot(&self) -> Vec<Vec<u8>> {
        self.spike.scratchpad_snapshot()
    }
}
