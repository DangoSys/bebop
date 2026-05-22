mod brk;
mod clock_gettime;
mod close;
mod exit;
mod fcntl;
mod fstat;
mod getcwd;
mod getrandom;
mod ioctl;
mod lseek;
mod mmap;
mod mprotect;
mod openat;
mod prlimit64;
mod read;
mod readlinkat;
mod riscv_hwprobe;
mod rt_sigaction;
mod rt_sigprocmask;
mod set_robust_list;
mod tgkill;
mod write;
mod writev;

use crate::constants::*;

pub use brk::handle_brk;
pub use clock_gettime::handle_clock_gettime;
pub use close::handle_close;
pub use exit::handle_exit;
pub use fcntl::handle_fcntl;
pub use fstat::handle_fstat;
pub use getcwd::handle_getcwd;
pub use getrandom::handle_getrandom;
pub use ioctl::handle_ioctl;
pub use lseek::handle_lseek;
pub use mmap::handle_mmap;
pub use mprotect::handle_mprotect;
pub use openat::handle_openat;
pub use prlimit64::handle_prlimit64;
pub use read::handle_read;
pub use readlinkat::handle_readlinkat;
pub use riscv_hwprobe::handle_riscv_hwprobe;
pub use rt_sigaction::handle_rt_sigaction;
pub use rt_sigprocmask::handle_rt_sigprocmask;
pub use set_robust_list::handle_set_robust_list;
pub use tgkill::handle_tgkill;
pub use write::handle_write;
pub use writev::handle_writev;

/// Returns `(result, should_exit)`.
// RISC-V syscall ABI fixes the argument list at a0..a5; folding into a struct burdens every caller.
#[allow(clippy::too_many_arguments)]
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
    let trace = std::env::var("BEMU_STRACE").is_ok();

    // For openat, decode the path string for better trace output
    let openat_path = if trace && syscall_num == SYS_OPENAT && a1 >= 0x80000000 {
        let offset = (a1 - 0x80000000) as usize;
        let mut bytes = Vec::new();
        for i in 0..256 {
            if offset + i >= memory.len() { break; }
            let b = memory[offset + i];
            if b == 0 { break; }
            bytes.push(b);
        }
        String::from_utf8_lossy(&bytes).to_string()
    } else {
        String::new()
    };

    let mut state = crate::state::SYSCALL_STATE.lock().unwrap();

    let result = match syscall_num {
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
        SYS_FSTAT => handle_fstat(&state, a0 as i64, a1, memory),
        SYS_SET_TID_ADDRESS => (1, false),
        SYS_GETCWD => handle_getcwd(a0, a1 as usize, memory),
        SYS_FCNTL => handle_fcntl(a0 as i64, a1 as i32),
        SYS_IOCTL => handle_ioctl(a0 as i64, a1, a2, memory),
        SYS_GETPID | SYS_GETTID => (1, false),
        SYS_SET_ROBUST_LIST => handle_set_robust_list(a0, a1),
        SYS_FUTEX => (0, false),
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
    };

    if trace {
        let ret_signed = result.0 as i64;
        let name = syscall_name(syscall_num);
        if syscall_num == SYS_OPENAT {
            eprintln!(
                "[STRACE] {}({}, \"{}\", flags=0x{:x}, mode=0x{:x}) = {} ({})",
                name, a0 as i32, openat_path, a2, a3, ret_signed, result.0
            );
        } else {
            eprintln!(
                "[STRACE] {}(0x{:x}, 0x{:x}, 0x{:x}, 0x{:x}, 0x{:x}, 0x{:x}) = {} (0x{:x})",
                name, a0, a1, a2, a3, a4, a5, ret_signed, result.0
            );
        }
    }

    result
}

fn syscall_name(num: u64) -> &'static str {
    match num {
        SYS_GETCWD => "getcwd",
        SYS_FCNTL => "fcntl",
        SYS_IOCTL => "ioctl",
        SYS_OPENAT => "openat",
        SYS_CLOSE => "close",
        SYS_LSEEK => "lseek",
        SYS_READ => "read",
        SYS_WRITE => "write",
        SYS_WRITEV => "writev",
        SYS_READLINKAT => "readlinkat",
        SYS_FSTAT => "fstat",
        SYS_EXIT => "exit",
        SYS_EXIT_GROUP => "exit_group",
        SYS_SET_TID_ADDRESS => "set_tid_address",
        SYS_FUTEX => "futex",
        SYS_SET_ROBUST_LIST => "set_robust_list",
        SYS_CLOCK_GETTIME => "clock_gettime",
        SYS_TGKILL => "tgkill",
        SYS_RT_SIGACTION => "rt_sigaction",
        SYS_RT_SIGPROCMASK => "rt_sigprocmask",
        SYS_GETPID => "getpid",
        SYS_GETTID => "gettid",
        SYS_BRK => "brk",
        SYS_MUNMAP => "munmap",
        SYS_MMAP => "mmap",
        SYS_MPROTECT => "mprotect",
        SYS_RISCV_HWPROBE => "riscv_hwprobe",
        SYS_PRLIMIT64 => "prlimit64",
        SYS_GETRANDOM => "getrandom",
        SYS_RSEQ => "rseq",
        SYS_PREAD => "pread",
        SYS_PWRITE => "pwrite",
        _ => "unknown",
    }
}
