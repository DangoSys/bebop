use crate::constants::{ERR_FAULT, ERR_NOTTY};
use crate::utils::guest_range;

pub fn handle_ioctl(_fd: i64, req: u64, argp: u64, memory: &mut [u8]) -> (u64, bool) {
    if req == 0x5413 || req == 0x80085413 || req == 0x40085413 {
        let Some(off) = guest_range(argp, 8, memory.len()) else {
            return ((ERR_FAULT as u64), false);
        };
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
    if req == 0x802c542a {
        let Some(off) = guest_range(argp, 44, memory.len()) else {
            return ((ERR_FAULT as u64), false);
        };
        memory[off..off + 44].fill(0);
        let cflag: u32 = 0x000008b0;
        memory[off + 8..off + 12].copy_from_slice(&cflag.to_le_bytes());
        let speed: u32 = 38400;
        memory[off + 36..off + 40].copy_from_slice(&speed.to_le_bytes());
        memory[off + 40..off + 44].copy_from_slice(&speed.to_le_bytes());
        return (0, false);
    }
    ((ERR_NOTTY as u64), false)
}
