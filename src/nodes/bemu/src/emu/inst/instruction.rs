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

/// Per-instruction Bank access scoreboard for BEMU.
///
/// BEMU executes accelerator operations serially, but records the physical
/// Bank read/write dependencies exposed by each operation. The write set
/// returned at completion is authoritative for Bank-level DiffTest.
#[derive(Default)]
pub struct BankScoreboard {
    banks: RefCell<Vec<BankScoreboardEntry>>,
    instructions: RefCell<BTreeMap<u64, CompletedBankAccess>>,
}

#[derive(Default)]
struct BankScoreboardEntry {
    readers: BTreeSet<u64>,
    writers: BTreeSet<u64>,
}

#[derive(Default)]
pub struct CompletedBankAccess {
    pub reads: BTreeSet<usize>,
    pub writes: BTreeSet<usize>,
}

impl BankScoreboard {
    pub fn new(bank_num: usize) -> Self {
        Self {
            banks: RefCell::new((0..bank_num).map(|_| BankScoreboardEntry::default()).collect()),
            instructions: RefCell::new(BTreeMap::new()),
        }
    }

    pub fn reset(&self) {
        self.instructions.borrow_mut().clear();
        for entry in self.banks.borrow_mut().iter_mut() {
            *entry = BankScoreboardEntry::default();
        }
    }

    pub fn issue(&self, instruction_id: u64) {
        let old = self
            .instructions
            .borrow_mut()
            .insert(instruction_id, CompletedBankAccess::default());
        assert!(
            old.is_none(),
            "duplicate BEMU scoreboard issue for instruction {instruction_id}"
        );
    }

    pub fn record_read(&self, instruction_id: u64, bank_id: usize) {
        self.record(instruction_id, bank_id, false);
    }

    pub fn record_write(&self, instruction_id: u64, bank_id: usize) {
        self.record(instruction_id, bank_id, true);
    }

    fn record(&self, instruction_id: u64, bank_id: usize, is_write: bool) {
        let mut instructions = self.instructions.borrow_mut();
        let access = instructions
            .get_mut(&instruction_id)
            .unwrap_or_else(|| panic!("BEMU scoreboard access without issue: instruction {instruction_id}"));
        let mut banks = self.banks.borrow_mut();
        let entry = banks
            .get_mut(bank_id)
            .unwrap_or_else(|| panic!("BEMU scoreboard bank index out of range: {bank_id}"));
        if is_write {
            access.writes.insert(bank_id);
            entry.writers.insert(instruction_id);
        } else {
            access.reads.insert(bank_id);
            entry.readers.insert(instruction_id);
        }
    }

    /// Retire an operation and return the physical Banks it read and wrote.
    pub fn complete(&self, instruction_id: u64) -> CompletedBankAccess {
        let access = self
            .instructions
            .borrow_mut()
            .remove(&instruction_id)
            .unwrap_or_else(|| panic!("BEMU scoreboard completion without issue: instruction {instruction_id}"));
        let mut banks = self.banks.borrow_mut();
        for bank_id in &access.reads {
            banks[*bank_id].readers.remove(&instruction_id);
        }
        for bank_id in &access.writes {
            banks[*bank_id].writers.remove(&instruction_id);
        }
        access
    }
}

/// Bank storage used while executing one instruction.
///
/// Immutable and mutable indexing report physical Bank reads and writes to an
/// optional per-instruction scoreboard.
pub struct TrackedBanks<'a> {
    banks: &'a mut [Vec<u8>],
    scoreboard: Option<&'a BankScoreboard>,
    instruction_id: u64,
}

impl<'a> TrackedBanks<'a> {
    pub fn new(banks: &'a mut [Vec<u8>], scoreboard: Option<&'a BankScoreboard>, instruction_id: u64) -> Self {
        Self {
            banks,
            scoreboard,
            instruction_id,
        }
    }

    /// Borrow one Bank for reading and a distinct Bank for writing.
    pub fn read_write(&mut self, read_bank: usize, write_bank: usize) -> (&[u8], &mut [u8]) {
        assert_ne!(read_bank, write_bank, "Bank read/write pair must be distinct");
        self.record_read(read_bank);
        self.record_write(write_bank);

        if read_bank < write_bank {
            let (left, right) = self.banks.split_at_mut(write_bank);
            (&left[read_bank], &mut right[0])
        } else {
            let (left, right) = self.banks.split_at_mut(read_bank);
            (&right[0], &mut left[write_bank])
        }
    }

    /// Initialize a newly allocated Bank without reporting an architectural
    /// BankDataWrite event.
    pub fn initialize(&mut self, bank_id: usize, value: u8) {
        self.banks[bank_id].fill(value);
    }

    fn record_read(&self, bank_id: usize) {
        if let Some(scoreboard) = self.scoreboard {
            scoreboard.record_read(self.instruction_id, bank_id);
        }
    }

    fn record_write(&self, bank_id: usize) {
        if let Some(scoreboard) = self.scoreboard {
            scoreboard.record_write(self.instruction_id, bank_id);
        }
    }
}

impl Index<usize> for TrackedBanks<'_> {
    type Output = Vec<u8>;

    fn index(&self, index: usize) -> &Self::Output {
        self.record_read(index);
        &self.banks[index]
    }
}

impl IndexMut<usize> for TrackedBanks<'_> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.record_write(index);
        &mut self.banks[index]
    }
}

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
    pub banks: TrackedBanks<'a>,
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

#[cfg(test)]
mod tests {
    use super::{BankScoreboard, TrackedBanks};
    use std::collections::BTreeSet;

    #[test]
    fn scoreboard_identifies_only_written_physical_banks() {
        let mut storage = vec![vec![0u8; 4]; 3];
        let scoreboard = BankScoreboard::new(3);
        scoreboard.issue(7);
        let mut banks = TrackedBanks::new(&mut storage, Some(&scoreboard), 7);

        let _ = banks[0][0];
        banks[2][1] = 0;

        drop(banks);
        assert_eq!(scoreboard.complete(7).writes, BTreeSet::from([2]));
    }

    #[test]
    fn scoreboard_records_read_write_dependencies_but_returns_only_writes() {
        let mut storage = vec![vec![0u8; 4]; 3];
        let scoreboard = BankScoreboard::new(3);
        scoreboard.issue(8);
        let mut banks = TrackedBanks::new(&mut storage, Some(&scoreboard), 8);

        let (source, destination) = banks.read_write(2, 0);
        destination[1] = source[1];

        drop(banks);
        assert_eq!(scoreboard.complete(8).writes, BTreeSet::from([0]));
    }

    #[test]
    fn allocation_initialization_does_not_enter_writer_scoreboard() {
        let mut storage = vec![vec![1u8; 4]; 2];
        let scoreboard = BankScoreboard::new(2);
        scoreboard.issue(9);
        let mut banks = TrackedBanks::new(&mut storage, Some(&scoreboard), 9);

        banks.initialize(0, 0);

        drop(banks);
        assert!(scoreboard.complete(9).writes.is_empty());
    }

    #[test]
    fn scoreboard_tracking_can_be_disabled() {
        let mut storage = vec![vec![0u8; 4]; 2];
        let mut banks = TrackedBanks::new(&mut storage, None, 10);

        banks[1][0] = 2;
    }

    #[test]
    fn scoreboard_reset_discards_in_flight_dependencies() {
        let mut storage = vec![vec![0u8; 4]; 2];
        let scoreboard = BankScoreboard::new(2);
        scoreboard.issue(11);
        let mut banks = TrackedBanks::new(&mut storage, Some(&scoreboard), 11);
        banks[1][0] = 3;
        drop(banks);

        scoreboard.reset();
        scoreboard.issue(11);

        assert!(scoreboard.complete(11).writes.is_empty());
    }
}
