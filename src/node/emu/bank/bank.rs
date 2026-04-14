pub const BANK_NUM: usize = 32;
pub const BANK_WIDTH: usize = 128;
pub const BANK_LINES: usize = 1024;
pub const BANK_SIZE: usize = BANK_LINES * (BANK_WIDTH / 8);
pub const MATRIX_SIZE: usize = 16;

/// Matches `PrivateMemBackend.mappingTable`: physical SRAM bank slot -> currently bound virtual bank id.
#[derive(Clone, Default, Debug)]
pub struct MapEntry {
    pub valid: bool,
    pub vbank_id: u32,
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

    /// Equivalent to RTL `deleteEntry`: release all physical slots occupied by this vbank.
    pub fn delete_vbank(&mut self, v: u32) {
        for e in &mut self.slots {
            if e.valid && e.vbank_id == v {
                *e = MapEntry::default();
            }
        }
    }

    pub fn first_free_pbank(&self) -> Option<usize> {
        self.slots.iter().position(|e| !e.valid)
    }

    /// Bind physical slot `p` to virtual id `v` (allocation path should call `delete_vbank(v)` first).
    pub fn bind(&mut self, p: usize, v: u32) {
        self.slots[p].valid = true;
        self.slots[p].vbank_id = v;
    }

    /// Virtual bank id -> physical bank index (RTL resolves by matching `vbank_id` in entries).
    pub fn resolve(&self, v: u32) -> Option<usize> {
        self.slots.iter().position(|e| e.valid && e.vbank_id == v)
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct BankConfig {
    pub allocated: bool,
    pub cols: u64,
}

// #[inline]
// pub fn mem_read(mem: &[u8], addr: u64) -> u8 {
//     mem[(addr as usize) % mem.len()]
// }

// #[inline]
// pub fn mem_write(mem: &mut [u8], addr: u64, v: u8) {
//     mem[(addr as usize) % mem.len()] = v;
// }
