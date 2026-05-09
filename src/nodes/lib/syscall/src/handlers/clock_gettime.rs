use crate::constants::{ERR_FAULT, ERR_INVAL, GUEST_MEM_BASE};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn handle_clock_gettime(_clock_id: i32, tp_addr: u64, memory: &mut [u8]) -> (u64, bool) {
    if tp_addr < GUEST_MEM_BASE || tp_addr + 16 > GUEST_MEM_BASE + memory.len() as u64 {
        return ((ERR_FAULT as u64), false);
    }
    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d,
        Err(_) => return ((ERR_INVAL as u64), false),
    };
    let sec = now.as_secs() as i64;
    let nsec = now.subsec_nanos() as i64;
    let offset = (tp_addr - GUEST_MEM_BASE) as usize;
    memory[offset..offset + 8].copy_from_slice(&sec.to_le_bytes());
    memory[offset + 8..offset + 16].copy_from_slice(&nsec.to_le_bytes());
    (0, false)
}
