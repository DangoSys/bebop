use crate::constants::{GUEST_MEM_BASE, ERR_INVAL, ERR_FAULT};

pub fn handle_getcwd(buf_addr: u64, size: usize, memory: &mut [u8]) -> (u64, bool) {
    if size < 2 {
        return ((ERR_INVAL as u64), false);
    }
    let mem_end = GUEST_MEM_BASE + memory.len() as u64;
    if buf_addr < GUEST_MEM_BASE || buf_addr + size as u64 > mem_end {
        return ((ERR_FAULT as u64), false);
    }
    let cwd = b"/\0";
    let off = (buf_addr - GUEST_MEM_BASE) as usize;
    memory[off..off + cwd.len()].copy_from_slice(cwd);
    (buf_addr, false)
}
