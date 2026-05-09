use crate::constants::{ERR_FAULT, ERR_INVAL, GUEST_MEM_BASE};

pub fn handle_prlimit64(pid: u64, resource: u64, _new_limit: u64, old_limit: u64, memory: &mut [u8]) -> (u64, bool) {
    if pid != 0 {
        return ((ERR_INVAL as u64), false);
    }
    if old_limit != 0 {
        let mem_end = GUEST_MEM_BASE + memory.len() as u64;
        if old_limit < GUEST_MEM_BASE || old_limit + 16 > mem_end {
            return ((ERR_FAULT as u64), false);
        }
        let (cur, max) = match resource {
            3 => (8 * 1024 * 1024_u64, 8 * 1024 * 1024_u64),
            7 => (1024_u64, 4096_u64),
            _ => (1024_u64 * 1024_u64, 1024_u64 * 1024_u64),
        };
        let offset = (old_limit - GUEST_MEM_BASE) as usize;
        memory[offset..offset + 8].copy_from_slice(&cur.to_le_bytes());
        memory[offset + 8..offset + 16].copy_from_slice(&max.to_le_bytes());
    }
    (0, false)
}
