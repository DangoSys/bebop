use crate::state::SyscallState;
use crate::utils::guest_range;
use std::io::Read;

pub fn handle_read(state: &mut SyscallState, fd: u64, buf_addr: u64, count: usize, memory: &mut [u8]) -> (u64, bool) {
    if fd == 0 {
        (0, false)
    } else if let Some(file) = state.open_files.get_mut(&fd) {
        let Some(offset) = guest_range(buf_addr, count, memory.len()) else {
            return ((-1i64 as u64), false);
        };
        let buf = &mut memory[offset..offset + count];

        match file.read(buf) {
            Ok(n) => (n as u64, false),
            Err(_) => ((-1i64 as u64), false),
        }
    } else {
        ((-1i64 as u64), false)
    }
}
