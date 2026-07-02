use crate::constants::{
    ANON_RESERVE_COMMIT_LIMIT, ERR_INVAL, ERR_NOMEM, GUEST_MEM_BASE, MAP_ANONYMOUS, MAP_PRIVATE, MMAP_TOP_RESERVED,
    PAGE_SIZE,
};
use crate::state::SyscallState;
use crate::utils::{align_down, align_up};

// Linux mmap2 ABI dictates the argument count; folding into a struct just splits each call site.
#[allow(clippy::too_many_arguments)]
pub fn handle_mmap(
    state: &mut SyscallState,
    addr: u64,
    length: u64,
    _prot: u64,
    flags: u64,
    fd: i64,
    offset: u64,
    memory: &[u8],
) -> (u64, bool) {
    if length == 0 {
        return ((ERR_INVAL as u64), false);
    }
    let mem_low = if state.mem_low == 0 {
        GUEST_MEM_BASE
    } else {
        state.mem_low
    };
    let mem_end = if state.mem_high == 0 {
        GUEST_MEM_BASE + memory.len() as u64
    } else {
        state.mem_high
    };
    if state.brk_addr == 0 {
        state.brk_addr = align_up(mem_low + 0x20_0000, PAGE_SIZE);
    }
    if state.mmap_base == 0 {
        state.mmap_base = align_down(mem_end - MMAP_TOP_RESERVED, PAGE_SIZE);
    }
    let length_aligned = align_up(length, PAGE_SIZE);
    let is_anon_private =
        (flags & (MAP_PRIVATE | MAP_ANONYMOUS)) == (MAP_PRIVATE | MAP_ANONYMOUS) && fd == -1 && offset == 0;
    let commit_len = if is_anon_private && length_aligned > ANON_RESERVE_COMMIT_LIMIT {
        ANON_RESERVE_COMMIT_LIMIT
    } else {
        length_aligned
    };
    if commit_len > (mem_end - mem_low) {
        return ((ERR_NOMEM as u64), false);
    }
    if addr != 0 {
        let map_start = align_down(addr, PAGE_SIZE);
        let map_end = match map_start.checked_add(commit_len) {
            Some(v) => v,
            None => return ((ERR_NOMEM as u64), false),
        };
        if map_start < mem_low || map_end > mem_end {
            return ((ERR_NOMEM as u64), false);
        }
        return (map_start, false);
    }

    let next_base = match state.mmap_base.checked_sub(commit_len) {
        Some(v) => align_down(v, PAGE_SIZE),
        None => return ((ERR_NOMEM as u64), false),
    };
    if next_base <= state.brk_addr || next_base < mem_low {
        return ((ERR_NOMEM as u64), false);
    }
    state.mmap_base = next_base;
    (next_base, false)
}
