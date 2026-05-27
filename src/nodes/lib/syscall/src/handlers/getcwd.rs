use crate::constants::{ERR_FAULT, ERR_INVAL};
use crate::utils::guest_range;

pub fn handle_getcwd(buf_addr: u64, size: usize, memory: &mut [u8]) -> (u64, bool) {
    if size < 2 {
        return ((ERR_INVAL as u64), false);
    }
    let Some(off) = guest_range(buf_addr, size, memory.len()) else {
        return ((ERR_FAULT as u64), false);
    };

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

    memory[off..off + cwd.len()].copy_from_slice(&cwd);
    // Linux kernel's getcwd syscall returns the number of bytes written
    // (including the null terminator), NOT the buffer address.
    // glibc's wrapper converts this length to a buf pointer.
    (cwd.len() as u64, false)
}
