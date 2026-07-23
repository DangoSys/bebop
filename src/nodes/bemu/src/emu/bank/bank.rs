include!(concat!(env!("OUT_DIR"), "/memory_model.rs"));
pub const MATRIX_SIZE: usize = 16;
pub const LOGICAL_BANK_GROUPS: u32 = BANK_NUM as u32;
const PAGE_SIZE: u64 = 4096;
use std::sync::atomic::{AtomicU64, Ordering};

static FAST_VIRT_BASE: AtomicU64 = AtomicU64::new(0);
static FAST_PHYS_BASE: AtomicU64 = AtomicU64::new(0);
static FAST_LEN: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static ADDR_CACHE: std::cell::Cell<Option<(u64, usize)>> = const { std::cell::Cell::new(None) };
}

pub fn clear_addr_cache() {
    FAST_LEN.store(0, Ordering::Relaxed);
    ADDR_CACHE.with(|cache| cache.set(None));
}

pub fn set_fast_addr_map(virt: u64, phys: u64, len: u64) {
    FAST_VIRT_BASE.store(virt, Ordering::Relaxed);
    FAST_PHYS_BASE.store(phys, Ordering::Relaxed);
    FAST_LEN.store(len, Ordering::Release);
    ADDR_CACHE.with(|cache| cache.set(None));
}

/// Mirrors RTL `PrivateMemBackend.mappingTable`:
/// physical SRAM bank slot -> bound virtual bank id.
#[derive(Clone, Default, Debug)]
pub struct MapEntry {
    pub valid: bool,
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
        for e in &mut self.slots {
            if e.valid && e.vbank_id == v {
                *e = MapEntry::default();
            }
        }
    }

    pub fn first_free_pbank(&self) -> Option<usize> {
        self.slots.iter().position(|e| !e.valid)
    }

    pub fn bind_group(&mut self, p: usize, v: u32, group: u32) {
        self.slots[p].valid = true;
        self.slots[p].vbank_id = v;
        self.slots[p].group_id = group;
    }

    pub fn resolve(&self, v: u32) -> Option<usize> {
        self.resolve_group(v, 0)
    }

    pub fn resolve_group(&self, v: u32, group: u32) -> Option<usize> {
        self.slots
            .iter()
            .position(|e| e.valid && e.vbank_id == v && e.group_id == group)
    }

    pub fn logical_bank_for_pbank(&self, pbank: usize) -> Option<u32> {
        let entry = self.slots.get(pbank)?;
        entry.valid.then(|| logical_bank_id(entry.vbank_id, entry.group_id))
    }
}

pub const fn logical_bank_id(vbank_id: u32, group_id: u32) -> u32 {
    vbank_id * LOGICAL_BANK_GROUPS + group_id
}

#[derive(Default, Clone, Copy, Debug)]
pub struct BankConfig {
    pub allocated: bool,
    pub cols: u64,
}

/// DRAM is mapped at this base address from the guest's perspective.
/// Must match `DRAM_BASE` in spike.cc.
pub const DRAM_BASE: u64 = 0x80000000;

#[inline]
fn dram_offset(mem_len: usize, addr: u64) -> usize {
    let fast_len = FAST_LEN.load(Ordering::Acquire);
    if fast_len != 0 {
        let virt = FAST_VIRT_BASE.load(Ordering::Relaxed);
        if addr >= virt && addr < virt + fast_len {
            let phys = FAST_PHYS_BASE.load(Ordering::Relaxed) + (addr - virt);
            let off = (phys - DRAM_BASE) as usize;
            if off < mem_len {
                return off;
            }
        }
    }

    let page = addr & !(PAGE_SIZE - 1);
    let page_off = (addr - page) as usize;
    if let Some(off) = ADDR_CACHE.with(|cache| {
        cache.get().and_then(|(cached_page, cached_off)| {
            if cached_page == page {
                cached_off.checked_add(page_off)
            } else {
                None
            }
        })
    }) {
        if off < mem_len {
            return off;
        }
    }

    if let Some(off) = bebop_syscall::translate_guest_addr(addr, 1, mem_len) {
        if let Some(page_base_off) = off.checked_sub(page_off) {
            ADDR_CACHE.with(|cache| cache.set(Some((page, page_base_off))));
        }
        return off;
    }

    let mem_end = DRAM_BASE + mem_len as u64;
    if addr >= DRAM_BASE && addr < mem_end {
        return (addr - DRAM_BASE) as usize;
    }

    panic!(
        "DRAM access out of range: addr=0x{:x} (valid range 0x{:x}-0x{:x})",
        addr, DRAM_BASE, mem_end
    );
}

#[inline]
pub fn mem_read(mem: &[u8], addr: u64) -> u8 {
    mem[dram_offset(mem.len(), addr)]
}

#[inline]
pub fn mem_write(mem: &mut [u8], addr: u64, v: u8) {
    let off = dram_offset(mem.len(), addr);
    mem[off] = v;
}
