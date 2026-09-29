#[allow(unused_imports)]
pub use crate::config::{
    bank_lines, bank_num, bank_row_bytes, bank_size, bank_width, is_shared_vbank, mmio_bank_lines, mmio_bank_num,
    mmio_bank_row_bytes, mmio_bank_size, mmio_bank_width, mmio_enable, mmio_read_width, mmio_total_size,
    virtual_bank_num,
};

pub const MATRIX_SIZE: usize = 16;

/// Mirrors RTL `PrivateMemBackend.mappingTable`:
/// physical SRAM bank slot -> bound virtual bank id.
#[derive(Clone, Default, Debug)]
pub struct MapEntry {
    pub valid: bool,
    pub hart_id: usize,
    pub vbank_id: u32,
    pub group_id: u32,
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
