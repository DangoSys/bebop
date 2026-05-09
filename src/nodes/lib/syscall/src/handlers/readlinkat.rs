use crate::constants::{ERR_FAULT, ERR_INVAL, ERR_NOENT, GUEST_MEM_BASE};

pub fn handle_readlinkat(
    _dirfd: i64,
    path_addr: u64,
    buf_addr: u64,
    buf_size: usize,
    memory: &mut [u8],
) -> (u64, bool) {
    if path_addr < GUEST_MEM_BASE || buf_addr < GUEST_MEM_BASE || buf_size == 0 {
        return ((ERR_FAULT as u64), false);
    }
    if buf_addr + buf_size as u64 > GUEST_MEM_BASE + memory.len() as u64 {
        return ((ERR_FAULT as u64), false);
    }

    let path_offset = (path_addr - GUEST_MEM_BASE) as usize;
    let mut path_bytes = Vec::new();
    for i in 0..4096 {
        if path_offset + i >= memory.len() {
            return ((ERR_FAULT as u64), false);
        }
        let b = memory[path_offset + i];
        if b == 0 {
            break;
        }
        path_bytes.push(b);
    }
    let path = match std::str::from_utf8(&path_bytes) {
        Ok(s) => s,
        Err(_) => return ((ERR_INVAL as u64), false),
    };
    if path != "/proc/self/exe" {
        return ((ERR_NOENT as u64), false);
    }

    let exe = b"/proc/self/exe";
    let n = exe.len().min(buf_size);
    let buf_offset = (buf_addr - GUEST_MEM_BASE) as usize;
    memory[buf_offset..buf_offset + n].copy_from_slice(&exe[..n]);
    (n as u64, false)
}
