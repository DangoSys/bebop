use crate::constants::{GUEST_MEM_BASE, PAGE_SIZE};
use crate::state::SyscallState;
use crate::utils::{align_down, align_up};

pub fn handle_brk(state: &mut SyscallState, addr: u64, memory: &[u8]) -> (u64, bool) {
    let mem_end = GUEST_MEM_BASE + memory.len() as u64;
    if state.brk_addr == 0 {
        state.brk_addr = align_up(GUEST_MEM_BASE + 0x20_0000, PAGE_SIZE);
    }
    if state.mmap_base == 0 {
        state.mmap_base = align_down(mem_end - PAGE_SIZE, PAGE_SIZE);
    }
    if addr == 0 {
        (state.brk_addr, false)
    } else {
        if addr < GUEST_MEM_BASE || addr > mem_end || addr >= state.mmap_base {
            return (state.brk_addr, false);
        }
        state.brk_addr = addr;
        (addr, false)
    }
}
