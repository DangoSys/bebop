use crate::state::SyscallState;
use crate::utils::guest_range;
use bebop_memory::Memory;
use std::io::Read;

pub fn handle_read(state: &mut SyscallState, fd: u64, buf_addr: u64, count: usize, memory: &dyn Memory) -> (u64, bool) {
    let mut ranges = Vec::new();
    let mut position = 0;
    while position < count {
        let Some(address) = buf_addr.checked_add(position as u64) else {
            return ((-14_i64) as u64, false);
        };
        let bytes = (4096 - address as usize % 4096).min(count - position);
        let Some(offset) = guest_range(address, bytes, memory.len()) else {
            return ((-14_i64) as u64, false);
        };
        ranges.push((offset, bytes));
        position += bytes;
    }
    let mut buffer = vec![0; count];
    let result = if fd == 0 {
        std::io::stdin().lock().read(&mut buffer)
    } else if let Some(file) = state.open_files.get_mut(&fd) {
        file.read(&mut buffer)
    } else {
        return ((-9_i64) as u64, false);
    };
    match result {
        Ok(count) => {
            let mut position = 0;
            for (offset, bytes) in ranges {
                let bytes = bytes.min(count - position);
                memory.write_buffer(offset, &buffer[position..position + bytes]);
                position += bytes;
                if position == count {
                    break;
                }
            }
            (count as u64, false)
        }
        Err(error) => (
            (-i64::from(error.raw_os_error().expect("host I/O error must carry errno"))) as u64,
            false,
        ),
    }
}
