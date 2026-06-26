use crate::constants::GUEST_MEM_BASE;

pub fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}

pub fn align_down(value: u64, align: u64) -> u64 {
    value & !(align - 1)
}

pub fn guest_range(addr: u64, len: usize, mem_len: usize) -> Option<usize> {
    let end = addr.checked_add(len as u64)?;
    let high_end = GUEST_MEM_BASE.checked_add(mem_len as u64)?;
    if addr >= GUEST_MEM_BASE && end <= high_end {
        return Some((addr - GUEST_MEM_BASE) as usize);
    }

    None
}

pub fn guest_cstr(addr: u64, max_len: usize, memory: &[u8]) -> Option<Vec<u8>> {
    let start = guest_range(addr, 1, memory.len())?;
    let mut bytes = Vec::new();
    for i in 0..max_len {
        if start + i >= memory.len() {
            return None;
        }
        let b = memory[start + i];
        if b == 0 {
            return Some(bytes);
        }
        bytes.push(b);
    }
    None
}
