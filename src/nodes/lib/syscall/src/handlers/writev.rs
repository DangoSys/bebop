use super::write::handle_write;
use crate::constants::ERR_FAULT;
use crate::state::SyscallState;
use crate::utils::guest_range;
use bebop_memory::Memory;

pub fn handle_writev(
    state: &mut SyscallState,
    fd: u64,
    iov_addr: u64,
    iovcnt: usize,
    memory: &dyn Memory,
) -> (u64, bool) {
    let iovec_size = 16;
    let mut total_written = 0u64;

    for i in 0..iovcnt {
        let iov_offset = match iov_addr.checked_add((i * iovec_size) as u64) {
            Some(v) => v,
            None => return ((ERR_FAULT as u64), false),
        };

        let Some(mem_offset) = guest_range(iov_offset, iovec_size, memory.len()) else {
            return ((ERR_FAULT as u64), false);
        };

        let mut buf_ptr_bytes = [0u8; 8];
        let mut len_bytes = [0u8; 8];
        memory.read_buffer(mem_offset, &mut buf_ptr_bytes);
        memory.read_buffer(mem_offset + 8, &mut len_bytes);

        let buf_addr = u64::from_le_bytes(buf_ptr_bytes);
        let count = u64::from_le_bytes(len_bytes) as usize;

        if count == 0 {
            continue;
        }

        let (written, _) = handle_write(state, fd, buf_addr, count, memory);
        if (written as i64) < 0 {
            return (if total_written == 0 { written } else { total_written }, false);
        }
        total_written += written;
        if written < count as u64 {
            break;
        }
    }

    (total_written, false)
}
