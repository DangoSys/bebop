use crate::constants::{GUEST_MEM_BASE, LOW_ALIAS_BASE, PAGE_SIZE, ERR_INVAL, ERR_NOMEM};

pub fn handle_mprotect(addr: u64, len: u64, _prot: u64, memory: &[u8]) -> (u64, bool) {
    if len == 0 {
        return (0, false);
    }
    if addr & (PAGE_SIZE - 1) != 0 {
        return ((ERR_INVAL as u64), false);
    }
    let mem_len = memory.len() as u64;
    let mem_end = GUEST_MEM_BASE + mem_len;
    let low_end = LOW_ALIAS_BASE + mem_len;
    let end = match addr.checked_add(len) {
        Some(v) => v,
        None => return ((ERR_NOMEM as u64), false),
    };

    let high_ok = addr >= GUEST_MEM_BASE && end <= mem_end;
    let low_ok = addr >= LOW_ALIAS_BASE && end <= low_end;
    if !high_ok && !low_ok {
        return ((ERR_NOMEM as u64), false);
    }
    (0, false)
}
