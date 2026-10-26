pub use crate::configs::{
    bank_lines, bank_num, bank_row_bytes, bank_size, bank_width, is_shared_vbank, mmio_bank_lines, mmio_bank_num,
    mmio_bank_row_bytes, mmio_bank_size, mmio_bank_width, mmio_enable, mmio_read_width, mmio_total_size,
    virtual_bank_num,
};

pub const MATRIX_SIZE: usize = 16;

/// Physical bank slot bound to a virtual bank and its owning execution context.
#[derive(Clone, Default, Debug)]
pub struct MapEntry {
    pub valid: bool,
    pub hart_id: usize,
    pub vbank_id: u32,
    pub group_id: u32,
    pub leases: u32,
}

#[derive(Clone, Debug)]
pub struct BankMap {
    pub slots: Vec<MapEntry>,
}

impl BankMap {
    pub fn new(num_physical: usize) -> Self {
        Self {
            slots: vec![MapEntry::default(); num_physical],
        }
    }

    pub fn delete_vbank(&mut self, v: u32) {
        self.delete_hart_vbank(0, v)
    }

    pub fn delete_hart_vbank(&mut self, hart_id: usize, v: u32) {
        assert!(
            !self
                .slots
                .iter()
                .any(|e| e.valid && e.hart_id == hart_id && e.vbank_id == v && e.leases != 0),
            "cannot release or reallocate a leased shared bank"
        );
        for e in &mut self.slots {
            if e.valid && e.hart_id == hart_id && e.vbank_id == v {
                *e = MapEntry::default();
            }
        }
    }

    pub fn first_free_pbank(&self) -> Option<usize> {
        self.slots.iter().position(|e| !e.valid)
    }

    pub fn bind_group(&mut self, p: usize, v: u32, group: u32) {
        self.bind_hart_group(p, 0, v, group)
    }

    pub fn bind_hart_group(&mut self, p: usize, hart_id: usize, v: u32, group: u32) {
        assert_eq!(self.slots[p].leases, 0, "cannot transfer a leased shared bank");
        self.slots[p].valid = true;
        self.slots[p].hart_id = hart_id;
        self.slots[p].vbank_id = v;
        self.slots[p].group_id = group;
    }

    #[allow(dead_code)]
    pub fn resolve(&self, v: u32) -> Option<usize> {
        self.resolve_group(v, 0)
    }

    pub fn resolve_group(&self, v: u32, group: u32) -> Option<usize> {
        self.resolve_hart_group(0, v, group)
    }

    pub fn resolve_hart_group(&self, hart_id: usize, v: u32, group: u32) -> Option<usize> {
        self.slots
            .iter()
            .position(|e| e.valid && e.hart_id == hart_id && e.vbank_id == v && e.group_id == group)
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct BankConfig {
    pub allocated: bool,
    pub cols: u64,
    /// Rows populated by the most recent MVIN. Gemmini uses this to apply the
    /// CISC zero-op1-tail contract without guessing from bank contents.
    pub valid_rows: u64,
}

use std::cell::RefCell;
use std::collections::BTreeMap;

/// Per-instruction bank access scoreboard used by BEMU Golden Record
/// generation. Mutable bank access records an architectural write before the
/// actual bytes are modified, so idempotent writes are retained.
#[derive(Default)]
pub struct BankWrites {
    instructions: RefCell<BTreeMap<u64, InstructionBankAccess>>,
}

#[derive(Default)]
struct InstructionBankAccess {
    writes: BTreeSet<usize>,
    defines_value: bool,
}

impl BankWrites {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn issue(&self, inst_id: u64, defines_value: bool) {
        let old = self.instructions.borrow_mut().insert(
            inst_id,
            InstructionBankAccess {
                writes: BTreeSet::new(),
                defines_value,
            },
        );
        assert!(old.is_none(), "duplicate BEMU scoreboard instruction {inst_id}");
    }

    pub fn record_write(&self, inst_id: u64, physical_bank_id: usize) -> bool {
        let mut instructions = self.instructions.borrow_mut();
        let instruction = instructions
            .get_mut(&inst_id)
            .unwrap_or_else(|| panic!("BEMU bank write without scoreboard issue: instruction {inst_id}"));
        instruction.writes.insert(physical_bank_id) && instruction.defines_value
    }

    pub fn complete(&self, inst_id: u64) -> (bool, BTreeSet<usize>) {
        let instruction = self
            .instructions
            .borrow_mut()
            .remove(&inst_id)
            .unwrap_or_else(|| panic!("BEMU scoreboard completion without issue: instruction {inst_id}"));
        (instruction.defines_value, instruction.writes)
    }
}

use std::collections::BTreeSet;
use std::ops::{Index, IndexMut, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive};

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

    // A bank assignment defines a new value; unwritten bytes are outside that value.
    pub(super) fn define_value(&mut self) {
        self.initialized.fill(false);
        self.clear_hash();
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
impl_range_index!(
    RangeFrom<usize>,
    |r: &RangeFrom<usize>, _| r.start,
    |_: &RangeFrom<usize>, len| len
);
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
    pub(crate) banks: &'a mut [PrivateBank],
    shared_banks: Option<&'a mut [PrivateBank]>,
    scoreboard: Option<&'a BankWrites>,
    inst_id: u64,
    pub read_only: BTreeSet<usize>,
}

impl<'a> TrackedBanks<'a> {
    pub fn new(banks: &'a mut [PrivateBank], scoreboard: Option<&'a BankWrites>, inst_id: u64) -> Self {
        Self {
            banks,
            shared_banks: None,
            scoreboard,
            inst_id,
            read_only: BTreeSet::new(),
        }
    }

    pub fn with_shared(
        banks: &'a mut [PrivateBank],
        shared_banks: &'a mut [PrivateBank],
        scoreboard: Option<&'a BankWrites>,
        inst_id: u64,
    ) -> Self {
        Self {
            banks,
            shared_banks: Some(shared_banks),
            scoreboard,
            inst_id,
            read_only: BTreeSet::new(),
        }
    }

    pub fn shared_index(&self, physical_bank_id: usize) -> usize {
        self.banks.len() + physical_bank_id
    }

    fn record_write(&mut self, physical_bank_id: usize) {
        assert!(
            !self.read_only.contains(&physical_bank_id),
            "RVV kernel wrote a readonly bank"
        );
        if let Some(scoreboard) = self.scoreboard {
            if scoreboard.record_write(self.inst_id, physical_bank_id) {
                let private_count = self.banks.len();
                let bank = if physical_bank_id < private_count {
                    &mut self.banks[physical_bank_id]
                } else {
                    &mut self
                        .shared_banks
                        .as_deref_mut()
                        .expect("shared bank storage is unavailable")[physical_bank_id - private_count]
                };
                bank.define_value();
            }
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
    /// CoreState-local BB endpoint index, independent of task slots and CSR hart IDs.
    pub local_thread: usize,
    pub cfgs: &'a mut [BankConfig],
    pub bank_map: &'a mut BankMap,
    pub virtual_bank_count: usize,
}

//===- mmio.rs - MMIO bank read operations ---------------------------------===//
//
// Provides MMIO read functionality for Ball instructions.
// Balls can read per-element or per-block metadata (e.g., scales) from MMIO.
//
//===-----------------------------------------------------------------===//

/// Read a byte from MMIO banks.
///
/// # Arguments
/// * `mmio_banks` - MMIO banks sized from the active design memdomain TOML
/// * `addr` - Absolute byte address in the unified MMIO space
///
/// # Returns
/// The byte value at the specified MMIO address.
#[allow(dead_code)]
pub fn mmio_read_byte(mmio_banks: &[Vec<u8>], addr: usize) -> u8 {
    if !mmio_enable() {
        panic!("mmio_read_byte: MMIO is disabled for this BEMU design config");
    }

    if addr >= mmio_total_size() {
        panic!("mmio_read_byte: address 0x{:x} out of range", addr);
    }

    let bank_idx = addr % mmio_bank_num();
    let bank_offset = addr / mmio_bank_num();

    mmio_banks[bank_idx][bank_offset]
}
