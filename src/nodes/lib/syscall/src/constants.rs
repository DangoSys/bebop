// Standard Linux syscall numbers (RISC-V)
pub const SYS_GETCWD: u64 = 17;
pub const SYS_FCNTL: u64 = 25;
pub const SYS_IOCTL: u64 = 29;
pub const SYS_OPENAT: u64 = 56;
pub const SYS_CLOSE: u64 = 57;
pub const SYS_READLINKAT: u64 = 78;
pub const SYS_LSEEK: u64 = 62;
pub const SYS_READ: u64 = 63;
pub const SYS_WRITE: u64 = 64;
pub const SYS_WRITEV: u64 = 66;
pub const SYS_PREAD: u64 = 67;
pub const SYS_PWRITE: u64 = 68;
pub const SYS_FSTAT: u64 = 80;
pub const SYS_EXIT: u64 = 93;
pub const SYS_EXIT_GROUP: u64 = 94;
pub const SYS_SET_TID_ADDRESS: u64 = 96;
pub const SYS_CLOCK_GETTIME: u64 = 113;
pub const SYS_RT_SIGACTION: u64 = 134;
pub const SYS_RT_SIGPROCMASK: u64 = 135;
pub const SYS_TGKILL: u64 = 131;
pub const SYS_GETPID: u64 = 172;
pub const SYS_GETTID: u64 = 178;
pub const SYS_SET_ROBUST_LIST: u64 = 99;
pub const SYS_FUTEX: u64 = 98;
pub const SYS_BRK: u64 = 214;
pub const SYS_MUNMAP: u64 = 215;
pub const SYS_MMAP: u64 = 222;
pub const SYS_MPROTECT: u64 = 226;
pub const SYS_RISCV_HWPROBE: u64 = 258;
pub const SYS_PRLIMIT64: u64 = 261;
pub const SYS_GETRANDOM: u64 = 278;
pub const SYS_RSEQ: u64 = 293;

// Memory constants
pub const GUEST_MEM_BASE: u64 = 0x80000000;
pub const PAGE_SIZE: u64 = 4096;
pub const MMAP_TOP_RESERVED: u64 = 8 * 1024 * 1024;

// Error codes
pub const ERR_INVAL: i64 = -22;
pub const ERR_FAULT: i64 = -14;
pub const ERR_NOSYS: i64 = -38;
pub const ERR_NOMEM: i64 = -12;
pub const ERR_NOENT: i64 = -2;
pub const ERR_BADF: i64 = -9;
pub const ERR_NOTTY: i64 = -25;

// mmap flags
pub const MAP_PRIVATE: u64 = 0x02;
pub const MAP_ANONYMOUS: u64 = 0x20;
pub const ANON_RESERVE_COMMIT_LIMIT: u64 = 64 * 1024 * 1024;
