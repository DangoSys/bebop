use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;

// Linux syscall numbers (RISC-V)
const SYS_GETCWD: u64 = 17;
const SYS_FCNTL: u64 = 25;
const SYS_IOCTL: u64 = 29;
const SYS_OPENAT: u64 = 56;
const SYS_CLOSE: u64 = 57;
const SYS_LSEEK: u64 = 62;
const SYS_READ: u64 = 63;
const SYS_WRITE: u64 = 64;
const SYS_WRITEV: u64 = 66;
const SYS_PREAD: u64 = 67;
const SYS_PWRITE: u64 = 68;
const SYS_FSTAT: u64 = 80;
const SYS_EXIT: u64 = 93;
const SYS_EXIT_GROUP: u64 = 94;
const SYS_SET_TID_ADDRESS: u64 = 96;
const SYS_CLOCK_GETTIME: u64 = 113;
const SYS_BRK: u64 = 214;
const SYS_MUNMAP: u64 = 215;
const SYS_MMAP: u64 = 222;

static SYSCALL_STATE: Lazy<Mutex<SyscallState>> = Lazy::new(|| {
    Mutex::new(SyscallState::new())
});

struct SyscallState {
    open_files: HashMap<u64, File>,
    next_fd: u64,
    exit_code: Option<i32>,
    brk_addr: u64,
}

impl SyscallState {
    fn new() -> Self {
        let mut state = Self {
            open_files: HashMap::new(),
            next_fd: 3, // 0, 1, 2 are stdin, stdout, stderr
            exit_code: None,
            brk_addr: 0x80000000 + (1 << 30), // Start after 1GB DRAM
        };

        // We don't actually open stdin/stdout/stderr files here
        // They'll be handled specially in read/write

        state
    }

    fn alloc_fd(&mut self, file: File) -> u64 {
        let fd = self.next_fd;
        self.next_fd += 1;
        self.open_files.insert(fd, file);
        fd
    }
}

/// Handle a system call from the guest program
/// Returns (result, should_exit)
pub fn handle_syscall(
    syscall_num: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
    memory: &mut [u8],
) -> (u64, bool) {
    let mut state = SYSCALL_STATE.lock().unwrap();

    match syscall_num {
        SYS_WRITE => {
            let fd = a0;
            let buf_addr = a1;
            let count = a2 as usize;

            if fd == 1 || fd == 2 {
                // stdout or stderr - write to console
                if buf_addr < 0x80000000 || buf_addr + count as u64 > 0x80000000 + memory.len() as u64 {
                    return ((-1i64 as u64), false); // EFAULT
                }
                let offset = (buf_addr - 0x80000000) as usize;
                let data = &memory[offset..offset + count];

                if let Ok(s) = std::str::from_utf8(data) {
                    print!("{}", s);
                    std::io::stdout().flush().ok();
                } else {
                    // Binary data, just write it
                    std::io::stdout().write_all(data).ok();
                }
                (count as u64, false)
            } else {
                // File write
                if let Some(file) = state.open_files.get_mut(&fd) {
                    if buf_addr < 0x80000000 || buf_addr + count as u64 > 0x80000000 + memory.len() as u64 {
                        return ((-1i64 as u64), false);
                    }
                    let offset = (buf_addr - 0x80000000) as usize;
                    let data = &memory[offset..offset + count];

                    match file.write(data) {
                        Ok(n) => (n as u64, false),
                        Err(_) => ((-1i64 as u64), false),
                    }
                } else {
                    ((-1i64 as u64), false) // EBADF
                }
            }
        }

        SYS_READ => {
            let fd = a0;
            let buf_addr = a1;
            let count = a2 as usize;

            if fd == 0 {
                // stdin - not implemented yet
                (0, false)
            } else if let Some(file) = state.open_files.get_mut(&fd) {
                if buf_addr < 0x80000000 || buf_addr + count as u64 > 0x80000000 + memory.len() as u64 {
                    return ((-1i64 as u64), false);
                }
                let offset = (buf_addr - 0x80000000) as usize;
                let buf = &mut memory[offset..offset + count];

                match file.read(buf) {
                    Ok(n) => (n as u64, false),
                    Err(_) => ((-1i64 as u64), false),
                }
            } else {
                ((-1i64 as u64), false) // EBADF
            }
        }

        SYS_OPENAT => {
            let _dirfd = a0 as i32;
            let pathname_addr = a1;
            let flags = a2 as i32;
            let _mode = a3;

            // Read pathname from memory
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

            // Open file
            let mut opts = OpenOptions::new();
            if flags & 0x0001 != 0 { opts.write(true); } // O_WRONLY
            if flags & 0x0002 != 0 { opts.read(true).write(true); } // O_RDWR
            if flags & 0x0040 != 0 { opts.create(true); } // O_CREAT
            if flags & 0x0200 != 0 { opts.truncate(true); } // O_TRUNC
            if flags & 0x0400 != 0 { opts.append(true); } // O_APPEND
            if flags == 0 { opts.read(true); } // O_RDONLY

            match opts.open(path) {
                Ok(file) => {
                    let fd = state.alloc_fd(file);
                    (fd, false)
                }
                Err(_) => ((-1i64 as u64), false),
            }
        }

        SYS_CLOSE => {
            let fd = a0;
            if state.open_files.remove(&fd).is_some() {
                (0, false)
            } else {
                ((-1i64 as u64), false)
            }
        }

        SYS_LSEEK => {
            let fd = a0;
            let offset = a1 as i64;
            let whence = a2 as i32;

            if let Some(file) = state.open_files.get_mut(&fd) {
                let pos = match whence {
                    0 => SeekFrom::Start(offset as u64), // SEEK_SET
                    1 => SeekFrom::Current(offset),      // SEEK_CUR
                    2 => SeekFrom::End(offset),          // SEEK_END
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

        SYS_EXIT | SYS_EXIT_GROUP => {
            let exit_code = a0 as i32;
            state.exit_code = Some(exit_code);
            (0, true) // Signal to exit
        }

        SYS_BRK => {
            let addr = a0;
            if addr == 0 {
                // Query current brk
                (state.brk_addr, false)
            } else {
                // Set new brk
                state.brk_addr = addr;
                (addr, false)
            }
        }

        SYS_MMAP => {
            // Simple mmap implementation - just return an address
            let _addr = a0;
            let length = a1;
            let _prot = a2;
            let _flags = a3;
            let _fd = a4 as i64;
            let _offset = a5;

            let new_addr = state.brk_addr;
            state.brk_addr += length;
            (new_addr, false)
        }

        SYS_MUNMAP => {
            // No-op for now
            (0, false)
        }

        SYS_FSTAT | SYS_CLOCK_GETTIME | SYS_SET_TID_ADDRESS |
        SYS_IOCTL | SYS_FCNTL | SYS_GETCWD |
        SYS_PREAD | SYS_PWRITE | SYS_WRITEV => {
            // Stub implementations - return success or reasonable defaults
            (0, false)
        }

        _ => {
            eprintln!("Unimplemented syscall: {}", syscall_num);
            ((-1i64 as u64), false) // ENOSYS
        }
    }
}

pub fn get_exit_code() -> Option<i32> {
    SYSCALL_STATE.lock().unwrap().exit_code
}

pub fn reset_syscall_state() {
    let mut state = SYSCALL_STATE.lock().unwrap();
    *state = SyscallState::new();
}
