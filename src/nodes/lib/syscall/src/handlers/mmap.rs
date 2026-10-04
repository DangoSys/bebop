use crate::constants::{
    ERR_BADF, ERR_INVAL, ERR_NOMEM, GUEST_MEM_BASE, MAP_ANONYMOUS, MAP_PRIVATE, MMAP_TOP_RESERVED, PAGE_SIZE,
};
use crate::state::SyscallState;
use crate::utils::{align_down, align_up};
use bebop_memory::Memory;

const MAP_FIXED: u64 = 0x10;

#[allow(clippy::too_many_arguments)]
pub fn handle_mmap(
    state: &mut SyscallState,
    addr: u64,
    length: u64,
    prot: u64,
    flags: u64,
    fd: i64,
    offset: u64,
    memory: &dyn Memory,
) -> (u64, bool) {
    if length == 0
        || length.checked_add(PAGE_SIZE - 1).is_none()
        || prot & !7 != 0
        || flags & !(MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED) != 0
        || flags & MAP_PRIVATE == 0
        || offset % PAGE_SIZE != 0
        || (flags & MAP_FIXED != 0 && addr % PAGE_SIZE != 0)
    {
        return (ERR_INVAL as u64, false);
    }
    if flags & MAP_ANONYMOUS == 0 {
        let Some(file) = state.open_files.get(&(fd as u64)) else {
            return (ERR_BADF as u64, false);
        };
        match file.metadata() {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return (-19i64 as u64, false),
            Err(error) => return (-(error.raw_os_error().unwrap_or(5) as i64) as u64, false),
        }
        if let Err(error) = std::os::unix::fs::FileExt::read_at(file, &mut [0u8; 1], offset) {
            return (-(error.raw_os_error().unwrap_or(5) as i64) as u64, false);
        }
        if offset.checked_add(length).is_none() {
            return (ERR_INVAL as u64, false);
        }
    }
    let mem_low = if state.mem_low == 0 {
        GUEST_MEM_BASE
    } else {
        state.mem_low
    };
    let mem_end = if state.mem_high == 0 {
        GUEST_MEM_BASE + memory.len() as u64
    } else {
        state.mem_high
    };
    if state.brk_addr == 0 {
        state.brk_addr = align_up(mem_low + 0x20_0000, PAGE_SIZE);
    }
    if state.mmap_base == 0 {
        state.mmap_base = align_down(mem_end - MMAP_TOP_RESERVED, PAGE_SIZE);
    }
    let size = align_up(length, PAGE_SIZE);
    let start = if flags & MAP_FIXED != 0 {
        let Some(end) = addr.checked_add(size) else {
            return (ERR_NOMEM as u64, false);
        };
        // Fixed replacement is restricted to ranges allocated by this process's mmap.
        let mut cursor = addr;
        for &(base, bytes, protection) in &state.mmap_regions {
            if protection == 0 && base <= cursor && base + bytes > cursor {
                cursor = base + bytes;
            }
        }
        if cursor < end {
            return (ERR_NOMEM as u64, false);
        }
        addr
    } else {
        let Some(start) = state.mmap_base.checked_sub(size) else {
            return (ERR_NOMEM as u64, false);
        };
        if start <= state.brk_addr || start < mem_low {
            return (ERR_NOMEM as u64, false);
        }
        state.mmap_base = start;
        start
    };
    if start < mem_low || start.checked_add(size).is_none_or(|end| end > mem_end) {
        return (ERR_NOMEM as u64, false);
    }
    remove_mmap_region(state, start, size);
    state.mmap_regions.push((start, size, prot));
    state.mmap_regions.sort_unstable();
    (start, false)
}

pub fn handle_munmap(state: &mut SyscallState, addr: u64, length: u64) -> (u64, bool) {
    if addr % PAGE_SIZE != 0 || length == 0 || length.checked_add(PAGE_SIZE - 1).is_none() {
        return (ERR_INVAL as u64, false);
    }
    let size = align_up(length, PAGE_SIZE);
    if addr.checked_add(size).is_none() {
        return (ERR_INVAL as u64, false);
    }
    remove_mmap_region(state, addr, size);
    (0, false)
}

fn remove_mmap_region(state: &mut SyscallState, addr: u64, size: u64) {
    let end = addr + size;
    let mut retained = Vec::new();
    for (base, bytes, protection) in state.mmap_regions.drain(..) {
        let region_end = base + bytes;
        if base >= end || region_end <= addr {
            retained.push((base, bytes, protection));
        } else {
            if base < addr {
                retained.push((base, addr - base, protection));
            }
            if region_end > end {
                retained.push((end, region_end - end, protection));
            }
        }
    }
    state.mmap_regions = retained;
}
