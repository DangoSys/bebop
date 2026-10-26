//===--- run.rs ----- simulation run entry point ------------------------===//
//
// Copyright 2026 The Aerospace Corporation
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
// Why we implement this module here instead of in different modules for each target?
//   In sims directory we have a higher level of different tools. In order to
//   better manage the co-simulation between different tools, we place targets entries
//   in this module.
//
// In bebop, a simulation target is exclusively linked to a specific tool, because
// we believe that in current RTL simulation, one tool is bound to dominate. We
// consider that giving multiple tools equal standing in every scenario would
// result in excessive encapsulation.
//
//===----------------------------------------------------------------------===//

use crate::{P2eArgs, RunCommand, RunTarget, VerilatorArgs};
use snafu::{ResultExt, Whatever};

pub fn run(command: RunCommand) -> Result<(), Whatever> {
    match command.target {
        RunTarget::Vcs(args) => bebop_vcs::Simulator {
            executable: args.simv,
            dram_configs: args.dram_configs,
        }
        .run(&args.elf, &args.log_dir, args.wave.as_deref())
        .whatever_context("run VCS simulation"),
        RunTarget::Verilator(VerilatorArgs {
            elf,
            log_dir,
            no_wave,
            diff,
            fast,
            itrace,
            mtrace,
            pmctrace,
            ctrace,
            banktrace,
        }) => {
            #[cfg(feature = "verilator")]
            {
                crate::sims::verilator::run::run(crate::sims::verilator::run::VerilatorRunConfig {
                    elf,
                    log_dir,
                    wave: !no_wave,
                    diff,
                    fast,
                    trace: crate::sims::verilator::run::VerilatorTraceConfig {
                        itrace,
                        mtrace,
                        pmctrace,
                        ctrace,
                        banktrace,
                    },
                })
            }
            #[cfg(not(feature = "verilator"))]
            {
                let _ = (
                    elf, log_dir, no_wave, diff, fast, itrace, mtrace, pmctrace, ctrace, banktrace,
                );
                crate::sims::verilator::run::run_unavailable()
            }
        }
        RunTarget::P2e(P2eArgs {
            image,
            model_image,
            model_offset,
            bitstream,
            log_dir,
            fpga_location,
            multi_fpga,
            wave,
            wave_start,
            diff,
            image_elf,
            itrace,
            mtrace,
            pmctrace,
            ctrace,
            banktrace,
        }) => {
            #[cfg(feature = "p2e")]
            {
                crate::sims::p2e::run::run(crate::sims::p2e::run::P2eRunConfig {
                    image,
                    model: model_image.zip(model_offset),
                    bitstream,
                    log_dir,
                    fpga_location,
                    multi_fpga,
                    wave,
                    wave_start,
                    diff: diff.then(|| crate::sims::p2e::run::DiffConfig {
                        image_elf: image_elf.expect("--diff requires --image-elf"),
                    }),
                    trace: crate::sims::p2e::run::P2eTraceConfig {
                        itrace,
                        mtrace,
                        pmctrace,
                        ctrace,
                        banktrace,
                    },
                })
            }
            #[cfg(not(feature = "p2e"))]
            {
                let _ = (
                    image,
                    model_image,
                    model_offset,
                    bitstream,
                    log_dir,
                    fpga_location,
                    multi_fpga,
                    wave,
                    wave_start,
                    diff,
                    image_elf,
                    itrace,
                    mtrace,
                    pmctrace,
                    ctrace,
                    banktrace,
                );
                crate::sims::p2e::run::run_unavailable()
            }
        }
    }
}
