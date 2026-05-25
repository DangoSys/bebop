//===- instruction.rs - Instruction trait definition -----------------------===//
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
// Instruction trait enforces uniform interface for all instructions.
// Each instruction implements exec() and latency() methods.
//
// ExecContext bundles all mutable state (memory, banks, configs, bank_map)
// to simplify instruction signatures.
//
//===-----------------------------------------------------------------===//-----===//

use super::super::bank::{BankConfig, BankMap};

/// MMIO region descriptor
#[derive(Clone, Copy, Default)]
pub struct MmioRegion {
    pub valid: bool,
    pub mmio_addr: u16,
    pub size_rows: u8,
}

/// Execution context passed to all instructions
pub struct ExecContext<'a> {
    pub memory: &'a mut [u8],
    pub banks: &'a mut [Vec<u8>],
    pub cfgs: &'a mut [BankConfig],
    pub bank_map: &'a mut BankMap,
    pub mmio_banks: &'a mut [[u8; 1024]; 16],
    pub mmio_region_table: &'a mut [MmioRegion; 32],
}

/// Instruction trait - all instructions must implement this
pub trait Instruction {
    /// Instruction opcode (funct7 field)
    const FUNCT: u32;

    /// Execute the instruction, return result value
    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64;

    /// Calculate latency (cycles from issue to complete)
    fn latency(xs1: u64, xs2: u64) -> u64;
}
