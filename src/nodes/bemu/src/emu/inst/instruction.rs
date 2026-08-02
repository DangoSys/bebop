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
use std::collections::BTreeSet;
use std::ops::{Index, IndexMut};

/// Bank storage wrapper used to derive the architectural write-set.
/// Mutable indexing records a write even when the new bytes equal the old
/// bytes, which is required for Golden Records of idempotent writes.
pub struct TrackedBanks<'a> {
    banks: &'a mut [Vec<u8>],
    written: BTreeSet<usize>,
    tracking_enabled: bool,
}

impl<'a> TrackedBanks<'a> {
    pub fn new(banks: &'a mut [Vec<u8>], tracking_enabled: bool) -> Self {
        Self {
            banks,
            written: BTreeSet::new(),
            tracking_enabled,
        }
    }

    pub fn into_written(self) -> BTreeSet<usize> {
        self.written
    }

    /// Alias-safe access for operations that stream from one bank into a
    /// different destination bank.
    pub fn read_write(&mut self, read_bank: usize, write_bank: usize) -> (&[u8], &mut [u8]) {
        assert_ne!(read_bank, write_bank, "bank read/write pair must be distinct");
        if self.tracking_enabled {
            self.written.insert(write_bank);
        }
        if read_bank < write_bank {
            let (left, right) = self.banks.split_at_mut(write_bank);
            (&left[read_bank], &mut right[0])
        } else {
            let (left, right) = self.banks.split_at_mut(read_bank);
            (&right[0], &mut left[write_bank])
        }
    }

    /// Allocation-time clearing is configuration initialization, not an SPM
    /// result produced by an instruction.
    pub fn initialize(&mut self, bank_id: usize, value: u8) {
        self.banks[bank_id].fill(value);
    }
}

impl Index<usize> for TrackedBanks<'_> {
    type Output = Vec<u8>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.banks[index]
    }
}

impl IndexMut<usize> for TrackedBanks<'_> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        if self.tracking_enabled {
            self.written.insert(index);
        }
        &mut self.banks[index]
    }
}

/// MMIO region descriptor
#[allow(dead_code)]
#[derive(Clone, Copy, Default)]
pub struct MmioRegion {
    pub valid: bool,
    pub mmio_addr: u16,
    pub size_rows: u8,
}

/// Execution context passed to all instructions
pub struct ExecContext<'a> {
    pub memory: &'a mut [u8],
    pub banks: TrackedBanks<'a>,
    pub cfgs: &'a mut [BankConfig],
    pub bank_map: &'a mut BankMap,
    pub mmio_banks: &'a mut [Vec<u8>],
    pub mmio_region_table: &'a mut [MmioRegion],
}

#[cfg(test)]
mod tests {
    use super::TrackedBanks;
    use std::collections::BTreeSet;

    #[test]
    fn idempotent_mutation_is_still_an_architectural_write() {
        let mut storage = vec![vec![0u8; 4]; 2];
        let mut banks = TrackedBanks::new(&mut storage, true);
        banks[1][0] = 0;
        assert_eq!(banks.into_written(), BTreeSet::from([1]));
    }

    #[test]
    fn allocation_initialization_is_not_a_data_write() {
        let mut storage = vec![vec![1u8; 4]; 2];
        let mut banks = TrackedBanks::new(&mut storage, true);
        banks.initialize(0, 0);
        assert!(banks.into_written().is_empty());
    }
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
