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
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Index, IndexMut};

/// Per-instruction bank access scoreboard used by BEMU Golden Record
/// generation. Mutable bank access records an architectural write before the
/// actual bytes are modified, so idempotent writes are retained.
#[derive(Default)]
pub struct BankScoreboard {
    instructions: RefCell<BTreeMap<u64, InstructionBankAccess>>,
}

#[derive(Default)]
struct InstructionBankAccess {
    writes: BTreeSet<usize>,
}

impl BankScoreboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&self) {
        self.instructions.borrow_mut().clear();
    }

    pub fn issue(&self, instruction_id: u64) {
        let old = self
            .instructions
            .borrow_mut()
            .insert(instruction_id, InstructionBankAccess::default());
        assert!(old.is_none(), "duplicate BEMU scoreboard instruction {instruction_id}");
    }

    pub fn record_write(&self, instruction_id: u64, physical_bank_id: usize) {
        self.instructions
            .borrow_mut()
            .get_mut(&instruction_id)
            .unwrap_or_else(|| panic!("BEMU bank write without scoreboard issue: instruction {instruction_id}"))
            .writes
            .insert(physical_bank_id);
    }

    pub fn complete(&self, instruction_id: u64) -> BTreeSet<usize> {
        self.instructions
            .borrow_mut()
            .remove(&instruction_id)
            .unwrap_or_else(|| panic!("BEMU scoreboard completion without issue: instruction {instruction_id}"))
            .writes
    }
}

/// Bank storage wrapper that reports mutable bank access to the scoreboard.
pub struct TrackedBanks<'a> {
    banks: &'a mut [Vec<u8>],
    shared_banks: Option<&'a mut [Vec<u8>]>,
    scoreboard: Option<&'a BankScoreboard>,
    instruction_id: u64,
}

impl<'a> TrackedBanks<'a> {
    pub fn new(banks: &'a mut [Vec<u8>], scoreboard: Option<&'a BankScoreboard>, instruction_id: u64) -> Self {
        Self {
            banks,
            shared_banks: None,
            scoreboard,
            instruction_id,
        }
    }

    pub fn with_shared(
        banks: &'a mut [Vec<u8>],
        shared_banks: &'a mut [Vec<u8>],
        scoreboard: Option<&'a BankScoreboard>,
        instruction_id: u64,
    ) -> Self {
        Self {
            banks,
            shared_banks: Some(shared_banks),
            scoreboard,
            instruction_id,
        }
    }

    pub fn shared_index(&self, physical_bank_id: usize) -> usize {
        self.banks.len() + physical_bank_id
    }

    fn record_write(&self, physical_bank_id: usize) {
        if let Some(scoreboard) = self.scoreboard {
            scoreboard.record_write(self.instruction_id, physical_bank_id);
        }
    }

    /// Alias-safe access for instructions that read one bank and write a
    /// different bank.
    pub fn read_write(&mut self, read_bank: usize, write_bank: usize) -> (&[u8], &mut [u8]) {
        assert_ne!(read_bank, write_bank, "bank read/write pair must be distinct");
        self.record_write(write_bank);
        let private_count = self.banks.len();
        match (read_bank < private_count, write_bank < private_count) {
            (true, true) => split_read_write(self.banks, read_bank, write_bank),
            (false, false) => split_read_write(
                self.shared_banks
                    .as_deref_mut()
                    .expect("shared bank storage is unavailable"),
                read_bank - private_count,
                write_bank - private_count,
            ),
            (true, false) => (
                &self.banks[read_bank],
                &mut self
                    .shared_banks
                    .as_deref_mut()
                    .expect("shared bank storage is unavailable")[write_bank - private_count],
            ),
            (false, true) => (
                &self
                    .shared_banks
                    .as_deref()
                    .expect("shared bank storage is unavailable")[read_bank - private_count],
                &mut self.banks[write_bank],
            ),
        }
    }

    /// Storage clearing performed while allocating a bank is configuration
    /// initialization and does not produce a BankDataWrite record.
    pub fn initialize(&mut self, physical_bank_id: usize, value: u8) {
        if physical_bank_id < self.banks.len() {
            self.banks[physical_bank_id].fill(value);
        } else {
            let private_count = self.banks.len();
            self.shared_banks
                .as_deref_mut()
                .expect("shared bank storage is unavailable")[physical_bank_id - private_count]
                .fill(value);
        }
    }
}

fn split_read_write(banks: &mut [Vec<u8>], read_bank: usize, write_bank: usize) -> (&[u8], &mut [u8]) {
    if read_bank < write_bank {
        let (left, right) = banks.split_at_mut(write_bank);
        (&left[read_bank], &mut right[0])
    } else {
        let (left, right) = banks.split_at_mut(read_bank);
        (&right[0], &mut left[write_bank])
    }
}

impl Index<usize> for TrackedBanks<'_> {
    type Output = Vec<u8>;

    fn index(&self, index: usize) -> &Self::Output {
        if index < self.banks.len() {
            &self.banks[index]
        } else {
            &self
                .shared_banks
                .as_deref()
                .expect("shared bank storage is unavailable")[index - self.banks.len()]
        }
    }
}

impl IndexMut<usize> for TrackedBanks<'_> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.record_write(index);
        if index < self.banks.len() {
            &mut self.banks[index]
        } else {
            let private_count = self.banks.len();
            &mut self
                .shared_banks
                .as_deref_mut()
                .expect("shared bank storage is unavailable")[index - private_count]
        }
    }
}

pub struct SharedBankContext<'a> {
    pub cfgs: &'a mut [BankConfig],
    pub bank_map: &'a mut BankMap,
    pub hart_id: usize,
    pub virtual_bank_count: usize,
}

/// Execution context passed to all instructions
pub struct ExecContext<'a> {
    pub hart_id: usize,
    pub instruction_id: u64,
    pub memory: &'a mut [u8],
    pub banks: TrackedBanks<'a>,
    pub cfgs: &'a mut [BankConfig],
    pub bank_map: &'a mut BankMap,
    pub shared: Option<SharedBankContext<'a>>,
    /// Virtual banks released by a CISC instruction after its bank digest is
    /// sampled. Keeping the mapping alive until then preserves the logical
    /// identity of every physical bank written by the instruction.
    pub deferred_bank_frees: &'a mut Vec<u32>,
    pub mmio_banks: &'a mut [Vec<u8>],
    pub barrier_hit: &'a mut bool,
}

impl ExecContext<'_> {
    pub fn config(&self, bank_id: u64) -> &BankConfig {
        let index = usize::try_from(bank_id).expect("bank id exceeds usize");
        if crate::config::is_shared_vbank(bank_id) {
            let shared = self.shared.as_ref().expect("shared bank storage is unavailable");
            &shared.cfgs
                [shared.hart_id % (shared.cfgs.len() / shared.virtual_bank_count) * shared.virtual_bank_count + index]
        } else {
            &self.cfgs[index]
        }
    }

    pub fn config_mut(&mut self, bank_id: u64) -> &mut BankConfig {
        let index = usize::try_from(bank_id).expect("bank id exceeds usize");
        if crate::config::is_shared_vbank(bank_id) {
            let shared = self.shared.as_mut().expect("shared bank storage is unavailable");
            let core = shared.hart_id % (shared.cfgs.len() / shared.virtual_bank_count);
            &mut shared.cfgs[core * shared.virtual_bank_count + index]
        } else {
            &mut self.cfgs[index]
        }
    }

    pub fn physical_bank(&self, bank_id: u64, group: u64) -> usize {
        if crate::config::is_shared_vbank(bank_id) {
            let shared = self.shared.as_ref().expect("shared bank storage is unavailable");
            let physical = shared
                .bank_map
                .resolve_hart_group(shared.hart_id, bank_id as u32, group as u32)
                .unwrap_or_else(|| panic!("shared vbank {bank_id} group {group} not mapped"));
            self.banks.banks.len() + physical
        } else {
            self.bank_map
                .resolve_group(bank_id as u32, group as u32)
                .unwrap_or_else(|| panic!("vbank {bank_id} group {group} not mapped"))
        }
    }

    pub fn reported_physical_bank(&self, bank_id: u64, encoded: usize) -> u32 {
        if crate::config::is_shared_vbank(bank_id) {
            (encoded - self.banks.banks.len()) as u32
        } else {
            encoded as u32
        }
    }

    pub fn defer_bank_free(&mut self, bank_id: u64) {
        let bank_id = u32::try_from(bank_id).expect("deferred bank id exceeds u32");
        assert!(
            self.config(bank_id as u64).allocated,
            "deferred free: bank {bank_id} is not allocated"
        );
        assert!(
            !self.deferred_bank_frees.contains(&bank_id),
            "deferred free: duplicate bank {bank_id}"
        );
        self.deferred_bank_frees.push(bank_id);
    }
}

#[cfg(test)]
mod tests {
    use super::{BankScoreboard, TrackedBanks};
    use std::collections::BTreeSet;

    #[test]
    fn scoreboard_records_idempotent_mutable_access() {
        let mut storage = vec![vec![0u8; 4]; 2];
        let scoreboard = BankScoreboard::new();
        scoreboard.issue(7);
        let mut banks = TrackedBanks::new(&mut storage, Some(&scoreboard), 7);
        banks[1][0] = 0;
        drop(banks);
        assert_eq!(scoreboard.complete(7), BTreeSet::from([1]));
    }

    #[test]
    fn reads_and_allocation_initialization_do_not_record_writes() {
        let mut storage = vec![vec![1u8; 4]; 2];
        let scoreboard = BankScoreboard::new();
        scoreboard.issue(8);
        let mut banks = TrackedBanks::new(&mut storage, Some(&scoreboard), 8);
        let _ = banks[1][0];
        banks.initialize(0, 0);
        drop(banks);
        assert!(scoreboard.complete(8).is_empty());
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

/// Ball semantics are selected by the Core's ballISA mnemonic mapping.
/// Ball implementations must not own a numeric funct7 encoding.
pub trait BallInstruction {
    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64;
    fn latency(xs1: u64, xs2: u64) -> u64;
}
