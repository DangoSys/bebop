use crate::constants::{ERR_FAULT, ERR_INVAL};
use crate::utils::guest_range;
use bebop_memory::Memory;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn handle_clock_gettime(_clock_id: i32, tp_addr: u64, memory: &dyn Memory) -> (u64, bool) {
    let Some(offset) = guest_range(tp_addr, 16, memory.len()) else {
        return ((ERR_FAULT as u64), false);
    };
    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d,
        Err(_) => return ((ERR_INVAL as u64), false),
    };
    let sec = now.as_secs() as i64;
    let nsec = now.subsec_nanos() as i64;
    memory.write_buffer(offset, &sec.to_le_bytes());
    memory.write_buffer(offset + 8, &nsec.to_le_bytes());
    (0, false)
}
