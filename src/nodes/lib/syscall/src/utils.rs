use crate::constants::GUEST_MEM_BASE;
use once_cell::sync::Lazy;
use std::sync::Mutex;

#[derive(Clone, Copy)]
struct GuestMapping {
    virt: u64,
    phys: u64,
    len: u64,
}

static GUEST_MAPPINGS: Lazy<Mutex<Vec<GuestMapping>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}

pub fn align_down(value: u64, align: u64) -> u64 {
    value & !(align - 1)
}

pub fn guest_range(addr: u64, len: usize, mem_len: usize) -> Option<usize> {
    let end = addr.checked_add(len as u64)?;
    let mappings = GUEST_MAPPINGS.lock().ok()?;
    if len == 0 {
        if let Some(mapping) = mappings.iter().rev().find(|mapping| {
            mapping
                .virt
                .checked_add(mapping.len)
                .is_some_and(|map_end| addr >= mapping.virt && addr < map_end)
        }) {
            let phys = mapping.phys.checked_add(addr.checked_sub(mapping.virt)?)?;
            if phys < GUEST_MEM_BASE {
                return None;
            }
            let offset = phys.checked_sub(GUEST_MEM_BASE)? as usize;
            return (offset <= mem_len).then_some(offset);
        }
    }
    if mappings.iter().rev().any(|mapping| {
        mapping
            .virt
            .checked_add(mapping.len)
            .is_some_and(|map_end| addr >= mapping.virt && addr < map_end)
    }) {
        let mut cursor = addr;
        let mut remaining = len as u64;
        let mut phys_start: Option<u64> = None;
        while remaining != 0 {
            let mapping = mappings.iter().rev().find(|mapping| {
                mapping
                    .virt
                    .checked_add(mapping.len)
                    .is_some_and(|map_end| cursor >= mapping.virt && cursor < map_end)
            })?;
            let map_offset = cursor.checked_sub(mapping.virt)?;
            let phys = mapping.phys.checked_add(map_offset)?;
            if let Some(start) = phys_start {
                if phys != start.checked_add(cursor.checked_sub(addr)?)? {
                    return None;
                }
            } else {
                phys_start = Some(phys);
            }
            let chunk = remaining.min(mapping.len.checked_sub(map_offset)?);
            cursor = cursor.checked_add(chunk)?;
            remaining -= chunk;
        }
        let phys = phys_start?;
        if phys < GUEST_MEM_BASE {
            return None;
        }
        let offset = phys.checked_sub(GUEST_MEM_BASE)? as usize;
        if offset.checked_add(len)? <= mem_len {
            return Some(offset);
        }
        return None;
    }

    let high_end = GUEST_MEM_BASE.checked_add(mem_len as u64)?;
    if addr >= GUEST_MEM_BASE && end <= high_end {
        return Some((addr - GUEST_MEM_BASE) as usize);
    }

    None
}

pub fn translate_guest_addr(addr: u64, len: usize, mem_len: usize) -> Option<usize> {
    guest_range(addr, len, mem_len)
}

pub fn set_guest_mappings(mappings: &[(u64, u64, u64)]) {
    let mut current = GUEST_MAPPINGS.lock().unwrap();
    current.clear();
    current.extend(
        mappings
            .iter()
            .map(|&(virt, phys, len)| GuestMapping { virt, phys, len }),
    );
}

pub fn add_guest_mapping(virt: u64, phys: u64, len: u64) {
    GUEST_MAPPINGS.lock().unwrap().push(GuestMapping { virt, phys, len });
}

pub fn guest_cstr(addr: u64, max_len: usize, memory: &[u8]) -> Option<Vec<u8>> {
    let start = guest_range(addr, 1, memory.len())?;
    let mut bytes = Vec::new();
    for i in 0..max_len {
        if start + i >= memory.len() {
            return None;
        }
        let b = memory[start + i];
        if b == 0 {
            return Some(bytes);
        }
        bytes.push(b);
    }
    None
}
