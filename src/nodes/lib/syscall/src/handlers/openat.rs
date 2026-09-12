use crate::state::SyscallState;
use crate::utils::guest_cstr;
use std::fs::OpenOptions;
use std::path::Path;

pub fn handle_openat(
    state: &mut SyscallState,
    _dirfd: i32,
    pathname_addr: u64,
    flags: i32,
    _mode: u64,
    memory: &[u8],
) -> (u64, bool) {
    let Some(path_bytes) = guest_cstr(pathname_addr, 4096, memory) else {
        return ((-1i64 as u64), false);
    };

    let path = match std::str::from_utf8(&path_bytes) {
        Ok(s) => s,
        Err(_) => return ((-1i64 as u64), false),
    };

    let mut opts = OpenOptions::new();
    if flags & 0x0001 != 0 {
        opts.write(true);
    }
    if flags & 0x0002 != 0 {
        opts.read(true).write(true);
    }
    if flags & 0x0040 != 0 {
        opts.create(true);
    }
    if flags & 0x0200 != 0 {
        opts.truncate(true);
    }
    if flags & 0x0400 != 0 {
        opts.append(true);
    }
    if flags == 0 {
        opts.read(true);
    }

    let path = Path::new(path);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        state.working_dir.join(path)
    };

    match opts.open(path) {
        Ok(file) => {
            let fd = state.alloc_fd(file);
            (fd, false)
        }
        Err(_) => ((-1i64 as u64), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::GUEST_MEM_BASE;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn relative_path_uses_guest_working_directory() {
        let dir = std::env::temp_dir().join(format!(
            "bebop-syscall-openat-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("payload.bin"), b"payload").unwrap();

        let mut state = SyscallState::new();
        state.working_dir = dir.clone();
        let mut memory = vec![0; 32];
        memory[..12].copy_from_slice(b"payload.bin\0");

        let (fd, should_exit) = handle_openat(&mut state, -100, GUEST_MEM_BASE, 0, 0, &memory);

        assert_eq!(fd, 3);
        assert!(!should_exit);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
