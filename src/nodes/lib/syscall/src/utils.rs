use crate::constants::GUEST_MEM_BASE;
use bebop_memory::Memory;
use std::cell::RefCell;

#[derive(Clone, Copy)]
struct GuestMapping {
    virt: u64,
    phys: u64,
    len: u64,
}

thread_local! {
    // Process::syscall installs the executing guest's mappings before dispatch.
    static GUEST_MAPPINGS: RefCell<Vec<GuestMapping>> = const { RefCell::new(Vec::new()) };
}

pub fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}

pub fn align_down(value: u64, align: u64) -> u64 {
    value & !(align - 1)
}

pub fn guest_range(addr: u64, len: usize, mem_len: usize) -> Option<usize> {
    GUEST_MAPPINGS.with_borrow(|mappings| {
        let end = addr.checked_add(len as u64)?;
        for mapping in mappings.iter().rev() {
            let map_end = mapping.virt.checked_add(mapping.len)?;
            if addr < mapping.virt || end > map_end {
                continue;
            }
            let phys = mapping.phys.checked_add(addr - mapping.virt)?;
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
        if mappings.is_empty() && addr >= GUEST_MEM_BASE && end <= high_end {
            return Some((addr - GUEST_MEM_BASE) as usize);
        }

        None
    })
}

pub fn translate_guest_addr(addr: u64, len: usize, mem_len: usize) -> Option<usize> {
    guest_range(addr, len, mem_len)
}

pub fn set_guest_mappings(mappings: &[(u64, u64, u64)]) {
    GUEST_MAPPINGS.with_borrow_mut(|current| {
        current.clear();
        current.extend(
            mappings
                .iter()
                .map(|&(virt, phys, len)| GuestMapping { virt, phys, len }),
        );
    });
}

pub fn add_guest_mapping(virt: u64, phys: u64, len: u64) {
    GUEST_MAPPINGS.with_borrow_mut(|current| current.push(GuestMapping { virt, phys, len }));
}

pub fn guest_cstr(addr: u64, max_len: usize, memory: &dyn Memory) -> Option<Vec<u8>> {
    let start = guest_range(addr, 1, memory.len())?;
    let mut bytes = Vec::new();
    for i in 0..max_len {
        if start + i >= memory.len() {
            return None;
        }
        let mut byte = [0];
        memory.read_buffer(start + i, &mut byte);
        let b = byte[0];
        if b == 0 {
            return Some(bytes);
        }
        bytes.push(b);
    }
    None
}
