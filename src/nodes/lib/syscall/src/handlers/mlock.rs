use crate::constants::{ERR_INVAL, ERR_NOMEM, PAGE_SIZE};
use crate::utils::guest_range;
use bebop_memory::Memory;

pub fn handle_mlock(addr: u64, len: u64, memory: &dyn Memory) -> (u64, bool) {
    // Linux rounds unsigned length plus the initial page offset to page size.
    let bytes = len.wrapping_add(addr & (PAGE_SIZE - 1)).wrapping_add(PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let start = addr & !(PAGE_SIZE - 1);
    let Some(end) = start.checked_add(bytes) else {
        return (ERR_INVAL as u64, false);
    };
    let mut page = start;
    while page < end {
        if guest_range(page, PAGE_SIZE as usize, memory.len()).is_none() {
            return (ERR_NOMEM as u64, false);
        }
        page += PAGE_SIZE;
    }
    // Guest memory is never reclaimed and its mappings already have PTEs.
    (0, false)
}

pub fn handle_mlockall(flags: u64) -> (u64, bool) {
    const CURRENT: u64 = 1;
    const FUTURE: u64 = 2;
    if flags == 0 || flags & !(CURRENT | FUTURE) != 0 {
        return (ERR_INVAL as u64, false);
    }
    (0, false)
}
