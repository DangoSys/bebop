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
use std::ops::{Index, IndexMut, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive};

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

    pub fn issue(&self, inst_id: u64) {
        let old = self
            .instructions
            .borrow_mut()
            .insert(inst_id, InstructionBankAccess::default());
        assert!(old.is_none(), "duplicate BEMU scoreboard instruction {inst_id}");
    }

    pub fn record_write(&self, inst_id: u64, physical_bank_id: usize) {
        self.instructions
            .borrow_mut()
            .get_mut(&inst_id)
            .unwrap_or_else(|| panic!("BEMU bank write without scoreboard issue: instruction {inst_id}"))
            .writes
            .insert(physical_bank_id);
    }

    pub fn complete(&self, inst_id: u64) -> BTreeSet<usize> {
        self.instructions
            .borrow_mut()
            .remove(&inst_id)
            .unwrap_or_else(|| panic!("BEMU scoreboard completion without issue: instruction {inst_id}"))
            .writes
    }
}

pub struct PrivateBank {
    bytes: Vec<u8>,
    initialized: Vec<bool>,
    hash: Option<BankHashCache>,
}

struct BankHashCache {
    rows: Vec<u32>,
    status: u32,
    dirty: BTreeSet<usize>,
}

impl PrivateBank {
    pub fn new(size: usize, hash: bool) -> Self {
        Self {
            bytes: vec![0; size],
            initialized: vec![true; size],
            hash: hash.then(|| BankHashCache {
                rows: vec![0; size / 16],
                status: 0,
                dirty: BTreeSet::new(),
            }),
        }
    }

    pub fn enable_hash(&mut self) {
        if self.hash.is_none() {
            self.hash = Some(BankHashCache {
                rows: vec![0; self.bytes.len() / 16],
                status: 0,
                dirty: (0..self.bytes.len() / 16).collect(),
            });
        }
    }

    fn clear_hash(&mut self) {
        if let Some(hash) = &mut self.hash {
            hash.rows.fill(0);
            hash.status = 0;
            hash.dirty.clear();
        }
    }

    fn dirty(&mut self, start: usize, end: usize) {
        if let Some(hash) = &mut self.hash {
            hash.dirty.extend(start / 16..end.div_ceil(16));
        }
    }

    pub fn status_hash(&mut self) -> u32 {
        let dirty = match &mut self.hash {
            Some(hash) => std::mem::take(&mut hash.dirty),
            None => panic!("bank hash is not enabled"),
        };
        let updates: Vec<_> = dirty
            .into_iter()
            .map(|row| {
                let start = row * 16;
                let bytes: Vec<_> = (start..start + 16)
                    .map(|index| if self.initialized[index] { self.bytes[index] } else { 0 })
                    .collect();
                (row, bebop_bank_hash::bank_row_hash(row as u32, &bytes))
            })
            .collect();
        let hash = self.hash.as_mut().expect("bank hash is enabled");
        for (row, value) in updates {
            hash.status = hash.status.wrapping_sub(hash.rows[row]).wrapping_add(value);
            hash.rows[row] = value;
        }
        hash.status
    }

    pub fn allocate(&mut self, clear: bool) {
        if clear {
            self.bytes.fill(0);
        }
        self.initialized.fill(clear);
        self.clear_hash();
    }

    pub fn initialize(&mut self, value: u8) {
        self.bytes.fill(value);
        self.initialized.fill(true);
        self.clear_hash();
        if value != 0 {
            self.dirty(0, self.bytes.len());
        }
    }

    pub fn reset(&mut self) {
        self.bytes.fill(0);
        self.initialized.fill(true);
        self.clear_hash();
    }
}

impl std::ops::Deref for PrivateBank {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}

impl Index<usize> for PrivateBank {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &self.bytes[index]
    }
}

impl IndexMut<usize> for PrivateBank {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.dirty(index, index + 1);
        self.initialized[index] = true;
        &mut self.bytes[index]
    }
}

macro_rules! impl_range_index {
    ($range:ty, $start:expr, $end:expr) => {
        impl Index<$range> for PrivateBank {
            type Output = [u8];

            fn index(&self, index: $range) -> &Self::Output {
                &self.bytes[index]
            }
        }

        impl IndexMut<$range> for PrivateBank {
            fn index_mut(&mut self, index: $range) -> &mut Self::Output {
                let start = $start(&index, self.bytes.len());
                let end = $end(&index, self.bytes.len());
                self.dirty(start, end);
                self.initialized[index.clone()].fill(true);
                &mut self.bytes[index]
            }
        }
    };
}

impl_range_index!(Range<usize>, |r: &Range<usize>, _| r.start, |r: &Range<usize>, _| r.end);
impl_range_index!(RangeFrom<usize>, |r: &RangeFrom<usize>, _| r.start, |_: &RangeFrom<usize>, len| len);
impl_range_index!(RangeFull, |_: &RangeFull, _| 0, |_: &RangeFull, len| len);
impl_range_index!(
    RangeInclusive<usize>,
    |r: &RangeInclusive<usize>, _| *r.start(),
    |r: &RangeInclusive<usize>, _| *r.end() + 1
);
impl_range_index!(RangeTo<usize>, |_: &RangeTo<usize>, _| 0, |r: &RangeTo<usize>, _| r.end);
impl_range_index!(
    RangeToInclusive<usize>,
    |_: &RangeToInclusive<usize>, _| 0,
    |r: &RangeToInclusive<usize>, _| r.end + 1
);

/// Bank storage wrapper that reports mutable bank access to the scoreboard.
pub struct TrackedBanks<'a> {
    banks: &'a mut [PrivateBank],
    shared_banks: Option<&'a mut [PrivateBank]>,
    scoreboard: Option<&'a BankScoreboard>,
    inst_id: u64,
}

impl<'a> TrackedBanks<'a> {
    pub fn new(banks: &'a mut [PrivateBank], scoreboard: Option<&'a BankScoreboard>, inst_id: u64) -> Self {
        Self {
            banks,
            shared_banks: None,
            scoreboard,
            inst_id,
        }
    }

    pub fn with_shared(
        banks: &'a mut [PrivateBank],
        shared_banks: &'a mut [PrivateBank],
        scoreboard: Option<&'a BankScoreboard>,
        inst_id: u64,
    ) -> Self {
        Self {
            banks,
            shared_banks: Some(shared_banks),
            scoreboard,
            inst_id,
        }
    }

    pub fn shared_index(&self, physical_bank_id: usize) -> usize {
        self.banks.len() + physical_bank_id
    }

    fn record_write(&self, physical_bank_id: usize) {
        if let Some(scoreboard) = self.scoreboard {
            scoreboard.record_write(self.inst_id, physical_bank_id);
        }
    }

    pub fn allocate(&mut self, physical_bank_id: usize, clear: bool) {
        self.banks[physical_bank_id].allocate(clear);
    }

    /// Alias-safe access for instructions that read one bank and write a
    /// different bank.
    pub fn read_write(&mut self, read_bank: usize, write_bank: usize) -> (&PrivateBank, &mut PrivateBank) {
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
            self.banks[physical_bank_id].initialize(value);
        } else {
            let private_count = self.banks.len();
            self.shared_banks
                .as_deref_mut()
                .expect("shared bank storage is unavailable")[physical_bank_id - private_count]
                .initialize(value);
        }
    }
}

fn split_read_write(
    banks: &mut [PrivateBank],
    read_bank: usize,
    write_bank: usize,
) -> (&PrivateBank, &mut PrivateBank) {
    if read_bank < write_bank {
        let (left, right) = banks.split_at_mut(write_bank);
        (&left[read_bank], &mut right[0])
    } else {
        let (left, right) = banks.split_at_mut(read_bank);
        (&right[0], &mut left[write_bank])
    }
}

impl Index<usize> for TrackedBanks<'_> {
    type Output = PrivateBank;

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
    pub inst_id: u64,
    pub memory: &'a mut [u8],
    pub translate_dma: bool,
    pub banks: TrackedBanks<'a>,
    pub cfgs: &'a mut [BankConfig],
    pub bank_map: &'a mut BankMap,
    pub shared: Option<SharedBankContext<'a>>,
    /// Virtual banks released by a CISC instruction after its bank hash is
    /// sampled. Keeping the mapping alive until then preserves the logical
    /// identity of every physical bank written by the instruction.
    pub deferred_bank_frees: &'a mut Vec<u32>,
    pub mmio_banks: &'a mut [Vec<u8>],
    pub barrier_hit: &'a mut bool,
}

impl ExecContext<'_> {
    pub fn read_memory(&self, addr: u64) -> u8 {
        if self.translate_dma {
            crate::ffi::dma_read(addr)
        } else {
            super::super::bank::mem_read(self.memory, addr)
        }
    }

    pub fn write_memory(&mut self, addr: u64, value: u8) {
        if self.translate_dma {
            crate::ffi::dma_write(addr, value);
        } else {
            super::super::bank::mem_write(self.memory, addr, value);
        }
    }

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
