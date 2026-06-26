use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fs::File;
use std::sync::Mutex;

pub static SYSCALL_STATE: Lazy<Mutex<SyscallState>> = Lazy::new(|| Mutex::new(SyscallState::new()));

pub struct SyscallState {
    pub open_files: HashMap<u64, File>,
    pub next_fd: u64,
    pub exit_code: Option<i32>,
    pub brk_addr: u64,
    pub mmap_base: u64,
}

impl SyscallState {
    pub fn new() -> Self {
        Self {
            open_files: HashMap::new(),
            next_fd: 3, // 0, 1, 2 are stdin, stdout, stderr
            exit_code: None,
            brk_addr: 0,
            mmap_base: 0,
        }
    }

    pub fn alloc_fd(&mut self, file: File) -> u64 {
        let fd = self.next_fd;
        self.next_fd += 1;
        self.open_files.insert(fd, file);
        fd
    }

    pub fn init_mem_layout(&mut self, brk_start: u64, mmap_base: u64) {
        self.brk_addr = brk_start;
        self.mmap_base = mmap_base;
    }
}

pub fn get_exit_code() -> Option<i32> {
    SYSCALL_STATE.lock().unwrap().exit_code
}

pub fn reset_syscall_state() {
    let mut state = SYSCALL_STATE.lock().unwrap();
    *state = SyscallState::new();
}

pub fn init_mem_layout(brk_start: u64, mmap_base: u64) {
    let mut state = SYSCALL_STATE.lock().unwrap();
    state.init_mem_layout(brk_start, mmap_base);
}
