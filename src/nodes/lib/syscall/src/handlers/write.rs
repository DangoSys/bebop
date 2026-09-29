use crate::state::SyscallState;
use crate::utils::guest_range;
use std::io::Write;

pub fn handle_write(state: &mut SyscallState, fd: u64, buf_addr: u64, count: usize, memory: &[u8]) -> (u64, bool) {
    let mut data = Vec::with_capacity(count);
    while data.len() < count {
        let Some(address) = buf_addr.checked_add(data.len() as u64) else {
            return ((-14_i64) as u64, false);
        };
        let bytes = (4096 - address as usize % 4096).min(count - data.len());
        let Some(offset) = guest_range(address, bytes, memory.len()) else {
            return ((-14_i64) as u64, false);
        };
        data.extend_from_slice(&memory[offset..offset + bytes]);
    }
    let result = if fd == 1 {
        let mut output = std::io::stdout().lock();
        output.write(&data).and_then(|count| output.flush().map(|()| count))
    } else if fd == 2 {
        let mut output = std::io::stderr().lock();
        output.write(&data).and_then(|count| output.flush().map(|()| count))
    } else if let Some(file) = state.open_files.get_mut(&fd) {
        file.write(&data)
    } else {
        return ((-9_i64) as u64, false);
    };
    match result {
        Ok(count) => (count as u64, false),
        Err(error) => (
            (-i64::from(error.raw_os_error().expect("host I/O error must carry errno"))) as u64,
            false,
        ),
    }
}
