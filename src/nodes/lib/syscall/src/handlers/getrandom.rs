use crate::constants::ERR_FAULT;
use crate::utils::guest_range;

pub fn handle_getrandom(buf_addr: u64, len: usize, _flags: u64, memory: &mut [u8]) -> (u64, bool) {
    if len == 0 {
        return (0, false);
    }
    let Some(offset) = guest_range(buf_addr, len, memory.len()) else {
        return ((ERR_FAULT as u64), false);
    };
    for i in 0..len {
        memory[offset + i] = ((i as u8).wrapping_mul(73)).wrapping_add(17);
    }
    (len as u64, false)
}
