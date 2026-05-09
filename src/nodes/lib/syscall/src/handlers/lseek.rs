use crate::state::SyscallState;
use std::io::{Seek, SeekFrom};

pub fn handle_lseek(state: &mut SyscallState, fd: u64, offset: i64, whence: i32) -> (u64, bool) {
    if let Some(file) = state.open_files.get_mut(&fd) {
        let pos = match whence {
            0 => SeekFrom::Start(offset as u64),
            1 => SeekFrom::Current(offset),
            2 => SeekFrom::End(offset),
            _ => return ((-1i64 as u64), false),
        };

        match file.seek(pos) {
            Ok(n) => (n, false),
            Err(_) => ((-1i64 as u64), false),
        }
    } else {
        ((-1i64 as u64), false)
    }
}
