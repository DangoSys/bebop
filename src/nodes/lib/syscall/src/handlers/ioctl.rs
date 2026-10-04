use crate::constants::{ERR_FAULT, ERR_NOTTY};
use crate::utils::guest_range;
use bebop_memory::Memory;

pub fn handle_ioctl(_fd: i64, req: u64, argp: u64, memory: &dyn Memory) -> (u64, bool) {
    if req == 0x5413 || req == 0x80085413 || req == 0x40085413 {
        let Some(off) = guest_range(argp, 8, memory.len()) else {
            return ((ERR_FAULT as u64), false);
        };
        let ws_row: u16 = 24;
        let ws_col: u16 = 80;
        let ws_xpixel: u16 = 0;
        let ws_ypixel: u16 = 0;
        memory.write_buffer(off, &ws_row.to_le_bytes());
        memory.write_buffer(off + 2, &ws_col.to_le_bytes());
        memory.write_buffer(off + 4, &ws_xpixel.to_le_bytes());
        memory.write_buffer(off + 6, &ws_ypixel.to_le_bytes());
        return (0, false);
    }
    if req == 0x802c542a {
        let Some(off) = guest_range(argp, 44, memory.len()) else {
            return ((ERR_FAULT as u64), false);
        };
        memory.fill(off, 44, 0);
        let cflag: u32 = 0x000008b0;
        memory.write_buffer(off + 8, &cflag.to_le_bytes());
        let speed: u32 = 38400;
        memory.write_buffer(off + 36, &speed.to_le_bytes());
        memory.write_buffer(off + 40, &speed.to_le_bytes());
        return (0, false);
    }
    ((ERR_NOTTY as u64), false)
}
