use crate::constants::{GUEST_MEM_BASE, MMAP_TOP_RESERVED, PAGE_SIZE};
use crate::state::SyscallState;
use crate::utils::{align_down, align_up};

pub fn handle_brk(state: &mut SyscallState, addr: u64, memory: &[u8]) -> (u64, bool) {
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
    if addr == 0 {
        (state.brk_addr, false)
    } else {
        if addr < mem_low || addr > mem_end || addr >= state.mmap_base {
            return (state.brk_addr, false);
        }
        state.brk_addr = addr;
        (addr, false)
    }
}
