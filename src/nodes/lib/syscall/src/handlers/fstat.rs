use crate::constants::{ERR_BADF, ERR_FAULT};
use crate::state::SyscallState;
use crate::utils::guest_range;
use bebop_memory::Memory;

pub fn handle_fstat(state: &SyscallState, fd: i64, stat_addr: u64, memory: &dyn Memory) -> (u64, bool) {
    let stat_size = 112usize;
    let Some(off) = guest_range(stat_addr, stat_size, memory.len()) else {
        return ((ERR_FAULT as u64), false);
    };
    if fd < 0 {
        return ((ERR_BADF as u64), false);
    }
    memory.fill(off, stat_size, 0);

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

    memory.write_buffer(off + 16, &st_mode.to_le_bytes());
    memory.write_buffer(off + 20, &st_nlink.to_le_bytes());
    memory.write_buffer(off + 48, &st_size.to_le_bytes());
    memory.write_buffer(off + 56, &st_blksize.to_le_bytes());
    memory.write_buffer(off + 64, &st_blocks.to_le_bytes());
    (0, false)
}
