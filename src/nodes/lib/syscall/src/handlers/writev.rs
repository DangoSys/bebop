use std::io::Write;
use crate::constants::{ERR_FAULT, ERR_INVAL};
use crate::state::SyscallState;

pub fn handle_writev(
    state: &mut SyscallState,
    fd: u64,
    iov_addr: u64,
    iovcnt: usize,
    memory: &mut [u8],
) -> (u64, bool) {
    let iovec_size = 16;
    let mut total_written = 0u64;

    for i in 0..iovcnt {
        let iov_offset = match iov_addr.checked_add((i * iovec_size) as u64) {
            Some(v) => v,
            None => return ((ERR_FAULT as u64), false),
        };

        if iov_offset < 0x80000000
            || iov_offset.checked_add(iovec_size as u64).is_none()
            || iov_offset + iovec_size as u64 > 0x80000000 + memory.len() as u64
        {
            return ((ERR_FAULT as u64), false);
        }

        let mem_offset = (iov_offset - 0x80000000) as usize;

        let mut buf_ptr_bytes = [0u8; 8];
        let mut len_bytes = [0u8; 8];
        buf_ptr_bytes.copy_from_slice(&memory[mem_offset..mem_offset + 8]);
        len_bytes.copy_from_slice(&memory[mem_offset + 8..mem_offset + 16]);

        let buf_addr = u64::from_le_bytes(buf_ptr_bytes);
        let count = u64::from_le_bytes(len_bytes) as usize;

        if count == 0 {
            continue;
        }

        if fd == 1 || fd == 2 {
            if buf_addr < 0x80000000
                || buf_addr.checked_add(count as u64).is_none()
                || buf_addr + count as u64 > 0x80000000 + memory.len() as u64
            {
                return ((ERR_FAULT as u64), false);
            }
            let offset = (buf_addr - 0x80000000) as usize;
            let data = &memory[offset..offset + count];

            if let Ok(s) = std::str::from_utf8(data) {
                print!("{}", s);
                std::io::stdout().flush().ok();
            } else {
                std::io::stdout().write_all(data).ok();
            }
            total_written += count as u64;
        } else {
            if let Some(file) = state.open_files.get_mut(&fd) {
                if buf_addr < 0x80000000
                    || buf_addr.checked_add(count as u64).is_none()
                    || buf_addr + count as u64 > 0x80000000 + memory.len() as u64
                {
                    return ((ERR_FAULT as u64), false);
                }
                let offset = (buf_addr - 0x80000000) as usize;
                let data = &memory[offset..offset + count];

                match file.write(data) {
                    Ok(n) => total_written += n as u64,
                    Err(_) => return ((ERR_INVAL as u64), false),
                }
            } else {
                return ((ERR_INVAL as u64), false);
            }
        }
    }

    (total_written, false)
}
