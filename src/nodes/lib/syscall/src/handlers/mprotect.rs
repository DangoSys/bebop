use crate::constants::{ERR_INVAL, ERR_NOMEM, PAGE_SIZE};
use crate::utils::guest_range;

pub fn handle_mprotect(addr: u64, len: u64, _prot: u64, memory: &[u8]) -> (u64, bool) {
    if len == 0 {
        return (0, false);
    }
    if addr & (PAGE_SIZE - 1) != 0 {
        return ((ERR_INVAL as u64), false);
    }
    let Ok(len) = usize::try_from(len) else {
        return ((ERR_NOMEM as u64), false);
    };
    if addr.checked_add(len as u64).is_none() || guest_range(addr, len, memory.len()).is_none() {
        return ((ERR_NOMEM as u64), false);
    }
    (0, false)
}
