use crate::constants::{ERR_BADF, ERR_FAULT};
use crate::state::SyscallState;
use crate::utils::guest_range;

pub fn handle_fstat(state: &SyscallState, fd: i64, stat_addr: u64, memory: &mut [u8]) -> (u64, bool) {
    let stat_size = 112usize;
    let Some(off) = guest_range(stat_addr, stat_size, memory.len()) else {
        return ((ERR_FAULT as u64), false);
    };
    if fd < 0 {
        return ((ERR_BADF as u64), false);
    }
    memory[off..off + stat_size].fill(0);

    let st_mode: u32 = if fd <= 2 { 0x2000 | 0o666 } else { 0x8000 | 0o644 };
    let st_nlink: u32 = 1;
    let st_blksize: i32 = 4096;

    // Get real file size from open_files if fd >= 3
    let st_size: i64 = if fd >= 3 {
        state
            .open_files
            .get(&(fd as u64))
            .and_then(|file| file.metadata().ok())
            .map(|meta| meta.len() as i64)
            .unwrap_or(0)
    } else {
        0
    };

    let st_blocks: i64 = (st_size + 511) / 512;

    memory[off + 16..off + 20].copy_from_slice(&st_mode.to_le_bytes());
    memory[off + 20..off + 24].copy_from_slice(&st_nlink.to_le_bytes());
    memory[off + 48..off + 56].copy_from_slice(&st_size.to_le_bytes());
    memory[off + 56..off + 60].copy_from_slice(&st_blksize.to_le_bytes());
    memory[off + 64..off + 72].copy_from_slice(&st_blocks.to_le_bytes());
    (0, false)
}
