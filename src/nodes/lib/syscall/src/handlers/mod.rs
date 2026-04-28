mod write;
mod read;
mod openat;
mod readlinkat;
mod close;
mod lseek;
mod exit;
mod brk;
mod mmap;
mod mprotect;
mod fstat;
mod getcwd;
mod fcntl;
mod ioctl;
mod set_robust_list;
mod riscv_hwprobe;
mod prlimit64;
mod getrandom;
mod rt_sigaction;
mod rt_sigprocmask;
mod tgkill;
mod clock_gettime;
mod writev;

use crate::constants::*;

pub use write::handle_write;
pub use read::handle_read;
pub use openat::handle_openat;
pub use readlinkat::handle_readlinkat;
pub use close::handle_close;
pub use lseek::handle_lseek;
pub use exit::handle_exit;
pub use brk::handle_brk;
pub use mmap::handle_mmap;
pub use mprotect::handle_mprotect;
pub use fstat::handle_fstat;
pub use getcwd::handle_getcwd;
pub use fcntl::handle_fcntl;
pub use ioctl::handle_ioctl;
pub use set_robust_list::handle_set_robust_list;
pub use riscv_hwprobe::handle_riscv_hwprobe;
pub use prlimit64::handle_prlimit64;
pub use getrandom::handle_getrandom;
pub use rt_sigaction::handle_rt_sigaction;
pub use rt_sigprocmask::handle_rt_sigprocmask;
pub use tgkill::handle_tgkill;
pub use clock_gettime::handle_clock_gettime;
pub use writev::handle_writev;

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
    let mut state = crate::state::SYSCALL_STATE.lock().unwrap();

    match syscall_num {
        SYS_WRITE => handle_write(&mut state, a0, a1, a2 as usize, memory),
        SYS_READ => handle_read(&mut state, a0, a1, a2 as usize, memory),
        SYS_OPENAT => handle_openat(&mut state, a0 as i32, a1, a2 as i32, a3, memory),
        SYS_READLINKAT => handle_readlinkat(a0 as i64, a1, a2, a3 as usize, memory),
        SYS_CLOSE => handle_close(&mut state, a0),
        SYS_LSEEK => handle_lseek(&mut state, a0, a1 as i64, a2 as i32),
        SYS_EXIT | SYS_EXIT_GROUP => handle_exit(&mut state, a0 as i32),
        SYS_BRK => handle_brk(&mut state, a0, memory),
        SYS_MMAP => handle_mmap(&mut state, a0, a1, a2, a3, a4 as i64, a5, memory),
        SYS_MUNMAP => (0, false),
        SYS_MPROTECT => handle_mprotect(a0, a1, a2, memory),
        SYS_FSTAT => handle_fstat(a0 as i64, a1, memory),
        SYS_SET_TID_ADDRESS => (1, false),
        SYS_GETCWD => handle_getcwd(a0, a1 as usize, memory),
        SYS_FCNTL => handle_fcntl(a0 as i64, a1 as i32),
        SYS_IOCTL => handle_ioctl(a0 as i64, a1, a2, memory),
        SYS_GETPID | SYS_GETTID => (1, false),
        SYS_SET_ROBUST_LIST => handle_set_robust_list(a0, a1),
        SYS_RISCV_HWPROBE => handle_riscv_hwprobe(a0, a1 as usize, a2, a3, a4, memory),
        SYS_PRLIMIT64 => handle_prlimit64(a0, a1, a2, a3, memory),
        SYS_GETRANDOM => handle_getrandom(a0, a1 as usize, a2, memory),
        SYS_RT_SIGACTION => handle_rt_sigaction(a0, a1, a2, a3, memory),
        SYS_RT_SIGPROCMASK => handle_rt_sigprocmask(a0, a1, a2, a3, memory),
        SYS_TGKILL => handle_tgkill(&mut state, a0 as i64, a1 as i64, a2 as i64),
        SYS_RSEQ => ((ERR_NOSYS as u64), false),
        SYS_CLOCK_GETTIME => handle_clock_gettime(a0 as i32, a1, memory),
        SYS_WRITEV => handle_writev(&mut state, a0, a1, a2 as usize, memory),
        SYS_PREAD | SYS_PWRITE => (0, false),
        _ => ((ERR_NOSYS as u64), false),
    }
}
