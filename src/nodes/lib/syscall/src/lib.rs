use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use once_cell::sync::Lazy;

// Standard Linux syscall numbers (RISC-V)
const SYS_GETCWD: u64 = 17;
const SYS_FCNTL: u64 = 25;
const SYS_IOCTL: u64 = 29;
const SYS_OPENAT: u64 = 56;
const SYS_CLOSE: u64 = 57;
const SYS_READLINKAT: u64 = 78;
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
const SYS_RT_SIGACTION: u64 = 134;
const SYS_RT_SIGPROCMASK: u64 = 135;
const SYS_TGKILL: u64 = 131;
const SYS_GETPID: u64 = 172;
const SYS_GETTID: u64 = 178;
const SYS_SET_ROBUST_LIST: u64 = 99;
const SYS_BRK: u64 = 214;
const SYS_MUNMAP: u64 = 215;
const SYS_MMAP: u64 = 222;
const SYS_MPROTECT: u64 = 226;
const SYS_RISCV_HWPROBE: u64 = 258;
const SYS_PRLIMIT64: u64 = 261;
const SYS_GETRANDOM: u64 = 278;
const SYS_RSEQ: u64 = 293;
const GUEST_MEM_BASE: u64 = 0x80000000;
const LOW_ALIAS_BASE: u64 = 0x10000;
const PAGE_SIZE: u64 = 4096;
const ERR_INVAL: i64 = -22;
const ERR_FAULT: i64 = -14;
const ERR_NOSYS: i64 = -38;
const ERR_NOMEM: i64 = -12;
const ERR_NOENT: i64 = -2;
const ERR_BADF: i64 = -9;
const ERR_NOTTY: i64 = -25;
const MAP_PRIVATE: u64 = 0x02;
const MAP_ANONYMOUS: u64 = 0x20;
const ANON_RESERVE_COMMIT_LIMIT: u64 = 64 * 1024 * 1024;

static SYSCALL_STATE: Lazy<Mutex<SyscallState>> = Lazy::new(|| {
    Mutex::new(SyscallState::new())
});

struct SyscallState {
    open_files: HashMap<u64, File>,
    next_fd: u64,
    exit_code: Option<i32>,
    brk_addr: u64,
    mmap_base: u64,
}

fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}

fn align_down(value: u64, align: u64) -> u64 {
    value & !(align - 1)
}

impl SyscallState {
    fn new() -> Self {
        let mut state = Self {
            open_files: HashMap::new(),
            next_fd: 3, // 0, 1, 2 are stdin, stdout, stderr
            exit_code: None,
            brk_addr: 0,
            mmap_base: 0,
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
        // Standard Linux syscalls
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

        SYS_READLINKAT => {
            let _dirfd = a0 as i64;
            let path_addr = a1;
            let buf_addr = a2;
            let buf_size = a3 as usize;
            if path_addr < GUEST_MEM_BASE || buf_addr < GUEST_MEM_BASE || buf_size == 0 {
                return ((ERR_FAULT as u64), false);
            }
            if buf_addr + buf_size as u64 > GUEST_MEM_BASE + memory.len() as u64 {
                return ((ERR_FAULT as u64), false);
            }

            let path_offset = (path_addr - GUEST_MEM_BASE) as usize;
            let mut path_bytes = Vec::new();
            for i in 0..4096 {
                if path_offset + i >= memory.len() {
                    return ((ERR_FAULT as u64), false);
                }
                let b = memory[path_offset + i];
                if b == 0 {
                    break;
                }
                path_bytes.push(b);
            }
            let path = match std::str::from_utf8(&path_bytes) {
                Ok(s) => s,
                Err(_) => return ((ERR_INVAL as u64), false),
            };
            if path != "/proc/self/exe" {
                return ((ERR_NOENT as u64), false);
            }

            let exe = b"/proc/self/exe";
            let n = exe.len().min(buf_size);
            let buf_offset = (buf_addr - GUEST_MEM_BASE) as usize;
            memory[buf_offset..buf_offset + n].copy_from_slice(&exe[..n]);
            (n as u64, false)
        }

        SYS_CLOSE => {
            let fd = a0;
            // For invalid fds (including -1), just return success to avoid infinite loops
            if fd as i64 == -1 || fd <= 2 {
                // stdin/stdout/stderr or invalid fd - just return success
                (0, false)
            } else if state.open_files.remove(&fd).is_some() {
                (0, false)
            } else {
                // File not found, but return success anyway to avoid loops
                (0, false)
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
            let mem_end = GUEST_MEM_BASE + memory.len() as u64;
            if state.brk_addr == 0 {
                state.brk_addr = align_up(GUEST_MEM_BASE + 0x20_0000, PAGE_SIZE);
            }
            if state.mmap_base == 0 {
                state.mmap_base = align_down(mem_end - PAGE_SIZE, PAGE_SIZE);
            }
            let addr = a0;
            if addr == 0 {
                // Query current brk
                (state.brk_addr, false)
            } else {
                // Linux brk failure returns current break, not -errno
                if addr < GUEST_MEM_BASE || addr > mem_end || addr >= state.mmap_base {
                    return (state.brk_addr, false);
                }
                state.brk_addr = addr;
                (addr, false)
            }
        }

        SYS_MMAP => {
            let addr = a0;
            let length = a1;
            let _prot = a2;
            let flags = a3;
            let fd = a4 as i64;
            let offset = a5;

            if length == 0 {
                return ((ERR_INVAL as u64), false);
            }
            let mem_end = GUEST_MEM_BASE + memory.len() as u64;
            if state.brk_addr == 0 {
                state.brk_addr = align_up(GUEST_MEM_BASE + 0x20_0000, PAGE_SIZE);
            }
            if state.mmap_base == 0 {
                state.mmap_base = align_down(mem_end - PAGE_SIZE, PAGE_SIZE);
            }
            let length_aligned = align_up(length, PAGE_SIZE);
            let is_anon_private = (flags & (MAP_PRIVATE | MAP_ANONYMOUS)) == (MAP_PRIVATE | MAP_ANONYMOUS)
                && fd == -1
                && offset == 0;
            let commit_len = if is_anon_private && length_aligned > ANON_RESERVE_COMMIT_LIMIT {
                ANON_RESERVE_COMMIT_LIMIT
            } else {
                length_aligned
            };
            if commit_len > (mem_end - GUEST_MEM_BASE) {
                return ((ERR_NOMEM as u64), false);
            }
            if addr != 0 {
                let map_start = align_down(addr, PAGE_SIZE);
                let map_end = match map_start.checked_add(commit_len) {
                    Some(v) => v,
                    None => return ((ERR_NOMEM as u64), false),
                };
                if map_start < GUEST_MEM_BASE || map_end > mem_end {
                    return ((ERR_NOMEM as u64), false);
                }
                return (map_start, false);
            }

            let next_base = match state.mmap_base.checked_sub(commit_len) {
                Some(v) => align_down(v, PAGE_SIZE),
                None => return ((ERR_NOMEM as u64), false),
            };
            if next_base <= state.brk_addr || next_base < GUEST_MEM_BASE {
                return ((ERR_NOMEM as u64), false);
            }
            state.mmap_base = next_base;
            (next_base, false)
        }

        SYS_MUNMAP => {
            // No-op for now
            (0, false)
        }

        SYS_MPROTECT => {
            let addr = a0;
            let len = a1;
            let _prot = a2;
            if len == 0 {
                return (0, false);
            }
            if addr & (PAGE_SIZE - 1) != 0 {
                return ((ERR_INVAL as u64), false);
            }
            let mem_len = memory.len() as u64;
            let mem_end = GUEST_MEM_BASE + mem_len;
            let low_end = LOW_ALIAS_BASE + mem_len;
            let end = match addr.checked_add(len) {
                Some(v) => v,
                None => return ((ERR_NOMEM as u64), false),
            };

            let high_ok = addr >= GUEST_MEM_BASE && end <= mem_end;
            let low_ok = addr >= LOW_ALIAS_BASE && end <= low_end;
            if !high_ok && !low_ok {
                return ((ERR_NOMEM as u64), false);
            }
            (0, false)
        }

        SYS_FSTAT => {
            let fd = a0 as i64;
            let stat_addr = a1;
            let stat_size = 112usize;
            let mem_end = GUEST_MEM_BASE + memory.len() as u64;
            if stat_addr < GUEST_MEM_BASE || stat_addr + stat_size as u64 > mem_end {
                return ((ERR_FAULT as u64), false);
            }
            if fd < 0 {
                return ((ERR_BADF as u64), false);
            }
            let off = (stat_addr - GUEST_MEM_BASE) as usize;
            memory[off..off + stat_size].fill(0);
            // asm-generic 64-bit struct stat layout (112 bytes)
            let st_mode: u32 = if fd <= 2 { 0x2000 | 0o666 } else { 0x8000 | 0o644 };
            let st_nlink: u32 = 1;
            let st_blksize: i32 = 4096;
            let st_size: i64 = 0;
            let st_blocks: i64 = 0;
            memory[off + 16..off + 20].copy_from_slice(&st_mode.to_le_bytes());
            memory[off + 20..off + 24].copy_from_slice(&st_nlink.to_le_bytes());
            memory[off + 48..off + 56].copy_from_slice(&st_size.to_le_bytes());
            memory[off + 56..off + 60].copy_from_slice(&st_blksize.to_le_bytes());
            memory[off + 64..off + 72].copy_from_slice(&st_blocks.to_le_bytes());
            (0, false)
        }

        SYS_SET_TID_ADDRESS => {
            (1, false)
        }

        SYS_GETCWD => {
            let buf_addr = a0;
            let size = a1 as usize;
            if size < 2 {
                return ((ERR_INVAL as u64), false);
            }
            let mem_end = GUEST_MEM_BASE + memory.len() as u64;
            if buf_addr < GUEST_MEM_BASE || buf_addr + size as u64 > mem_end {
                return ((ERR_FAULT as u64), false);
            }
            let cwd = b"/\0";
            let off = (buf_addr - GUEST_MEM_BASE) as usize;
            memory[off..off + cwd.len()].copy_from_slice(cwd);
            (buf_addr, false)
        }

        SYS_FCNTL => {
            let _fd = a0 as i64;
            let cmd = a1 as i32;
            match cmd {
                1 => (0, false), // F_GETFD
                3 => (0, false), // F_GETFL
                _ => (0, false),
            }
        }

        SYS_IOCTL => {
            let _fd = a0 as i64;
            let req = a1;
            let argp = a2;
            let mem_end = GUEST_MEM_BASE + memory.len() as u64;
            // TIOCGWINSZ and common encoded variants
            if req == 0x5413 || req == 0x80085413 || req == 0x40085413 {
                let end = match argp.checked_add(8) {
                    Some(v) => v,
                    None => return ((ERR_FAULT as u64), false),
                };
                if argp < GUEST_MEM_BASE || end > mem_end {
                    return ((ERR_FAULT as u64), false);
                }
                let off = (argp - GUEST_MEM_BASE) as usize;
                let ws_row: u16 = 24;
                let ws_col: u16 = 80;
                let ws_xpixel: u16 = 0;
                let ws_ypixel: u16 = 0;
                memory[off..off + 2].copy_from_slice(&ws_row.to_le_bytes());
                memory[off + 2..off + 4].copy_from_slice(&ws_col.to_le_bytes());
                memory[off + 4..off + 6].copy_from_slice(&ws_xpixel.to_le_bytes());
                memory[off + 6..off + 8].copy_from_slice(&ws_ypixel.to_le_bytes());
                return (0, false);
            }
            // TCGETS2 (termios2), often used by glibc startup probing.
            if req == 0x802c542a {
                let end = match argp.checked_add(44) {
                    Some(v) => v,
                    None => return ((ERR_FAULT as u64), false),
                };
                if argp < GUEST_MEM_BASE || end > mem_end {
                    return ((ERR_FAULT as u64), false);
                }
                let off = (argp - GUEST_MEM_BASE) as usize;
                memory[off..off + 44].fill(0);
                let cflag: u32 = 0x000008b0; // CREAD | CS8 | B38400 (common tty defaults)
                memory[off + 8..off + 12].copy_from_slice(&cflag.to_le_bytes());
                let speed: u32 = 38400;
                memory[off + 36..off + 40].copy_from_slice(&speed.to_le_bytes()); // c_ispeed
                memory[off + 40..off + 44].copy_from_slice(&speed.to_le_bytes()); // c_ospeed
                return (0, false);
            }
            ((ERR_NOTTY as u64), false)
        }

        SYS_GETPID | SYS_GETTID => {
            (1, false)
        }

        SYS_SET_ROBUST_LIST => {
            let _head = a0;
            let len = a1;
            if len != 24 {
                return ((-1i64 as u64), false);
            }
            (0, false)
        }

        SYS_RISCV_HWPROBE => {
            let pairs_addr = a0;
            let pair_count = a1 as usize;
            let _cpu_count = a2;
            let _cpus = a3;
            let flags = a4;
            if flags != 0 {
                return ((-1i64 as u64), false);
            }
            let pair_size = 16usize;
            if pair_count > 0 {
                if pairs_addr < GUEST_MEM_BASE {
                    return ((-1i64 as u64), false);
                }
                let total_size = match pair_count.checked_mul(pair_size) {
                    Some(v) => v as u64,
                    None => return ((-1i64 as u64), false),
                };
                let mem_end = GUEST_MEM_BASE + memory.len() as u64;
                if pairs_addr + total_size > mem_end {
                    return ((-1i64 as u64), false);
                }

                for i in 0..pair_count {
                    let item_addr = pairs_addr + (i * pair_size) as u64;
                    let item_offset = (item_addr - GUEST_MEM_BASE) as usize;
                    let mut key_bytes = [0u8; 8];
                    key_bytes.copy_from_slice(&memory[item_offset..item_offset + 8]);
                    let key = i64::from_le_bytes(key_bytes);

                    let (new_key, value): (i64, u64) = match key {
                        0 => (0, 0),
                        1 => (1, 0),
                        2 => (2, 0),
                        3 => (3, 1),
                        4 => (4, (1 << 0) | (1 << 1)),
                        5 => (5, 0),
                        _ => (-1, 0),
                    };

                    memory[item_offset..item_offset + 8].copy_from_slice(&new_key.to_le_bytes());
                    memory[item_offset + 8..item_offset + 16].copy_from_slice(&value.to_le_bytes());
                }
            }
            (0, false)
        }

        SYS_PRLIMIT64 => {
            let pid = a0;
            let resource = a1;
            let _new_limit = a2;
            let old_limit = a3;
            if pid != 0 {
                return ((ERR_INVAL as u64), false);
            }
            if old_limit != 0 {
                let mem_end = GUEST_MEM_BASE + memory.len() as u64;
                if old_limit < GUEST_MEM_BASE || old_limit + 16 > mem_end {
                    return ((ERR_FAULT as u64), false);
                }
                let (cur, max) = match resource {
                    3 => (8 * 1024 * 1024_u64, 8 * 1024 * 1024_u64),
                    7 => (1024_u64, 4096_u64),
                    _ => (1024_u64 * 1024_u64, 1024_u64 * 1024_u64),
                };
                let offset = (old_limit - GUEST_MEM_BASE) as usize;
                memory[offset..offset + 8].copy_from_slice(&cur.to_le_bytes());
                memory[offset + 8..offset + 16].copy_from_slice(&max.to_le_bytes());
            }
            (0, false)
        }

        SYS_GETRANDOM => {
            let buf_addr = a0;
            let len = a1 as usize;
            let _flags = a2;
            if len == 0 {
                return (0, false);
            }
            let mem_end = GUEST_MEM_BASE + memory.len() as u64;
            if buf_addr < GUEST_MEM_BASE || buf_addr + len as u64 > mem_end {
                return ((ERR_FAULT as u64), false);
            }
            let offset = (buf_addr - GUEST_MEM_BASE) as usize;
            for i in 0..len {
                memory[offset + i] = ((i as u8).wrapping_mul(73)).wrapping_add(17);
            }
            (len as u64, false)
        }

        SYS_RT_SIGACTION => {
            let signum = a0;
            let _act = a1;
            let oldact = a2;
            let sigsetsize = a3;
            if signum == 0 || signum > 64 {
                return ((-1i64 as u64), false);
            }
            if sigsetsize != 8 {
                return ((-1i64 as u64), false);
            }
            if oldact != 0 {
                let oldact_size = 32u64;
                if oldact < GUEST_MEM_BASE || oldact + oldact_size > GUEST_MEM_BASE + memory.len() as u64 {
                    return ((-1i64 as u64), false);
                }
                let offset = (oldact - GUEST_MEM_BASE) as usize;
                memory[offset..offset + oldact_size as usize].fill(0);
            }
            (0, false)
        }

        SYS_RT_SIGPROCMASK => {
            let how = a0;
            let _set = a1;
            let oldset = a2;
            let sigsetsize = a3;
            if how > 2 {
                return ((-1i64 as u64), false);
            }
            if sigsetsize != 8 {
                return ((-1i64 as u64), false);
            }
            if oldset != 0 {
                if oldset < GUEST_MEM_BASE || oldset + sigsetsize > GUEST_MEM_BASE + memory.len() as u64 {
                    return ((-1i64 as u64), false);
                }
                let offset = (oldset - GUEST_MEM_BASE) as usize;
                memory[offset..offset + sigsetsize as usize].fill(0);
            }
            (0, false)
        }

        SYS_TGKILL => {
            let tgid = a0 as i64;
            let tid = a1 as i64;
            let sig = a2 as i64;
            if tgid != 1 || tid != 1 || sig < 0 || sig > 64 {
                return ((-1i64 as u64), false);
            }
            if sig == 0 {
                return (0, false);
            }
            state.exit_code = Some(128 + sig as i32);
            (0, true)
        }

        SYS_RSEQ => {
            ((ERR_NOSYS as u64), false)
        }

        SYS_CLOCK_GETTIME => {
            let _clock_id = a0 as i32;
            let tp_addr = a1;
            if tp_addr < GUEST_MEM_BASE || tp_addr + 16 > GUEST_MEM_BASE + memory.len() as u64 {
                return ((ERR_FAULT as u64), false);
            }
            let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(d) => d,
                Err(_) => return ((ERR_INVAL as u64), false),
            };
            let sec = now.as_secs() as i64;
            let nsec = now.subsec_nanos() as i64;
            let offset = (tp_addr - GUEST_MEM_BASE) as usize;
            memory[offset..offset + 8].copy_from_slice(&sec.to_le_bytes());
            memory[offset + 8..offset + 16].copy_from_slice(&nsec.to_le_bytes());
            (0, false)
        }

        SYS_WRITEV => {
            // writev: write data from multiple buffers
            let fd = a0;
            let iov_addr = a1;
            let iovcnt = a2 as usize;

            // iovec structure: { void *iov_base; size_t iov_len; }
            // On 64-bit: 8 bytes for pointer + 8 bytes for length = 16 bytes per iovec
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

                // Read iovec structure
                let mut buf_ptr_bytes = [0u8; 8];
                let mut len_bytes = [0u8; 8];
                buf_ptr_bytes.copy_from_slice(&memory[mem_offset..mem_offset + 8]);
                len_bytes.copy_from_slice(&memory[mem_offset + 8..mem_offset + 16]);

                let buf_addr = u64::from_le_bytes(buf_ptr_bytes);
                let count = u64::from_le_bytes(len_bytes) as usize;

                if count == 0 {
                    continue;
                }

                // Write this buffer
                if fd == 1 || fd == 2 {
                    // stdout or stderr
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
                    // File write
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

        SYS_PREAD | SYS_PWRITE => {
            // Stub implementations - return success or reasonable defaults
            (0, false)
        }

        _ => {
            ((ERR_NOSYS as u64), false)
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
