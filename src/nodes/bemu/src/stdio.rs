use crate::root::platform::Platform;
use bebop_syscall::{translate_guest_addr, SYS_READ, SYS_WRITE, SYS_WRITEV};
use memory::Memory;
use std::{
    io::{Read, Write},
    sync::Mutex,
};

pub(crate) struct Streams {
    input: Box<dyn Read + Send>,
    output: Box<dyn Write + Send>,
}

impl Streams {
    pub(crate) fn new() -> Self {
        Self {
            input: Box::new(std::io::stdin()),
            output: Box::new(std::io::stdout()),
        }
    }

    pub(crate) fn syscall(
        &mut self,
        number: u64,
        fd: u64,
        address: u64,
        count: usize,
        platform: &Mutex<Platform>,
    ) -> Option<u64> {
        let result = if number == SYS_READ && fd == 0 {
            let memory_len = platform.lock().expect("BEMU platform poisoned").memory.len();
            let Some(ranges) = ranges(address, count, memory_len) else {
                return Some((-14_i64) as u64);
            };
            let mut bytes = vec![0; count];
            // Blocking host I/O must never hold the shared chip DDR lock.
            self.input.read(&mut bytes).map(|read| {
                let platform = platform.lock().expect("BEMU platform poisoned");
                let mut position = 0;
                for (offset, length) in ranges {
                    let length = length.min(read - position);
                    let memory: &dyn Memory = platform.memory.as_ref();
                    memory.write_buffer(offset, &bytes[position..position + length]);
                    position += length;
                    if position == read {
                        break;
                    }
                }
                read
            })
        } else if matches!(number, SYS_WRITE | SYS_WRITEV) && (fd == 1 || fd == 2) {
            let bytes = {
                let platform = platform.lock().expect("BEMU platform poisoned");
                let memory: &dyn Memory = platform.memory.as_ref();
                if number == SYS_WRITE {
                    collect(memory, address, count)
                } else {
                    let Some(size) = count.checked_mul(16) else {
                        return Some((-22_i64) as u64);
                    };
                    collect(memory, address, size).and_then(|iov| {
                        let mut bytes = Vec::new();
                        for item in iov.chunks_exact(16) {
                            let address = u64::from_le_bytes(item[..8].try_into().unwrap());
                            let length = u64::from_le_bytes(item[8..].try_into().unwrap()) as usize;
                            bytes.extend(collect(memory, address, length)?);
                        }
                        Some(bytes)
                    })
                }
            };
            let Some(bytes) = bytes else {
                return Some((-14_i64) as u64);
            };
            let mut stderr = std::io::stderr();
            let writer: &mut dyn Write = if fd == 1 { self.output.as_mut() } else { &mut stderr };
            let result = writer
                .write(&bytes)
                .and_then(|written| writer.flush().map(|()| written));
            result
        } else {
            return None;
        };
        Some(match result {
            Ok(count) => count as u64,
            Err(error) => (-i64::from(error.raw_os_error().expect("stdio error must carry errno"))) as u64,
        })
    }
}

fn ranges(address: u64, count: usize, memory_len: usize) -> Option<Vec<(usize, usize)>> {
    let mut ranges = Vec::new();
    let mut position = 0;
    while position < count {
        let current = address.checked_add(position as u64)?;
        let bytes = (4096 - current as usize % 4096).min(count - position);
        ranges.push((translate_guest_addr(current, bytes, memory_len)?, bytes));
        position += bytes;
    }
    Some(ranges)
}

fn collect(memory: &dyn Memory, address: u64, count: usize) -> Option<Vec<u8>> {
    let ranges = ranges(address, count, memory.len())?;
    let mut bytes = Vec::with_capacity(count);
    for (offset, length) in ranges {
        let start = bytes.len();
        bytes.resize(start + length, 0);
        memory.read_buffer(offset, &mut bytes[start..]);
    }
    Some(bytes)
}
