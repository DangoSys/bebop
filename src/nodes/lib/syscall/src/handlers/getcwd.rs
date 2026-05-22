use crate::constants::{ERR_FAULT, ERR_INVAL, GUEST_MEM_BASE};

pub fn handle_getcwd(buf_addr: u64, size: usize, memory: &mut [u8]) -> (u64, bool) {
    if size < 2 {
        return ((ERR_INVAL as u64), false);
    }
    let mem_end = GUEST_MEM_BASE + memory.len() as u64;
    if buf_addr < GUEST_MEM_BASE || buf_addr + size as u64 > mem_end {
        return ((ERR_FAULT as u64), false);
    }

    // Return the host's real CWD so guest programs can resolve relative paths correctly
    let cwd = match std::env::current_dir() {
        Ok(path) => {
            let mut path_bytes = path.to_string_lossy().as_bytes().to_vec();
            path_bytes.push(0); // null terminator
            path_bytes
        }
        Err(_) => b"/\0".to_vec(),
    };

    if cwd.len() > size {
        return ((ERR_INVAL as u64), false);
    }

    let off = (buf_addr - GUEST_MEM_BASE) as usize;
    memory[off..off + cwd.len()].copy_from_slice(&cwd);
    // Linux kernel's getcwd syscall returns the number of bytes written
    // (including the null terminator), NOT the buffer address.
    // glibc's wrapper converts this length to a buf pointer.
    (cwd.len() as u64, false)
}
