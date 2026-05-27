use crate::state::SyscallState;
use crate::utils::guest_range;
use std::io::Write;

pub fn handle_write(state: &mut SyscallState, fd: u64, buf_addr: u64, count: usize, memory: &[u8]) -> (u64, bool) {
    if fd == 1 || fd == 2 {
        let Some(offset) = guest_range(buf_addr, count, memory.len()) else {
            return ((-1i64 as u64), false);
        };
        let data = &memory[offset..offset + count];

        if let Ok(s) = std::str::from_utf8(data) {
            print!("{}", s);
            std::io::stdout().flush().ok();
        } else {
            std::io::stdout().write_all(data).ok();
        }
        (count as u64, false)
    } else if let Some(file) = state.open_files.get_mut(&fd) {
        let Some(offset) = guest_range(buf_addr, count, memory.len()) else {
            return ((-1i64 as u64), false);
        };
        let data = &memory[offset..offset + count];

        match file.write(data) {
            Ok(n) => (n as u64, false),
            Err(_) => ((-1i64 as u64), false),
        }
    } else {
        ((-1i64 as u64), false)
    }
}
