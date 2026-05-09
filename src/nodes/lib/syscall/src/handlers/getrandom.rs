use crate::constants::{ERR_FAULT, GUEST_MEM_BASE};

pub fn handle_getrandom(buf_addr: u64, len: usize, _flags: u64, memory: &mut [u8]) -> (u64, bool) {
    if len == 0 {
        return (0, false);
    }
    let mem_end = GUEST_MEM_BASE + memory.len() as u64;
    if buf_addr < GUEST_MEM_BASE || buf_addr + len as u64 > mem_end {
        return ((ERR_FAULT as u64), false);
    }
    let offset = (buf_addr - GUEST_MEM_BASE) as usize;
    for i in 0..len {
        memory[offset + i] = ((i as u8).wrapping_mul(73)).wrapping_add(17);
    }
    (len as u64, false)
}
