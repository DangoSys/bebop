use std::fs::OpenOptions;
use crate::state::SyscallState;

pub fn handle_openat(
    state: &mut SyscallState,
    _dirfd: i32,
    pathname_addr: u64,
    flags: i32,
    _mode: u64,
    memory: &[u8],
) -> (u64, bool) {
    if pathname_addr < 0x80000000 {
        return ((-1i64 as u64), false);
    }
    let offset = (pathname_addr - 0x80000000) as usize;
    let mut path_bytes = Vec::new();
    for i in 0..4096 {
        if offset + i >= memory.len() {
            return ((-1i64 as u64), false);
        }
        let b = memory[offset + i];
        if b == 0 {
            break;
        }
        path_bytes.push(b);
    }

    let path = match std::str::from_utf8(&path_bytes) {
        Ok(s) => s,
        Err(_) => return ((-1i64 as u64), false),
    };

    let mut opts = OpenOptions::new();
    if flags & 0x0001 != 0 { opts.write(true); }
    if flags & 0x0002 != 0 { opts.read(true).write(true); }
    if flags & 0x0040 != 0 { opts.create(true); }
    if flags & 0x0200 != 0 { opts.truncate(true); }
    if flags & 0x0400 != 0 { opts.append(true); }
    if flags == 0 { opts.read(true); }

    match opts.open(path) {
        Ok(file) => {
            let fd = state.alloc_fd(file);
            (fd, false)
        }
        Err(_) => ((-1i64 as u64), false),
    }
}
