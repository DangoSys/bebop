use crate::constants::GUEST_MEM_BASE;
use once_cell::sync::Lazy;
use std::sync::Mutex;

#[derive(Clone, Copy)]
struct GuestMapping {
    virt: u64,
    phys: u64,
    len: u64,
}

static GUEST_MAPPINGS: Lazy<Mutex<Vec<GuestMapping>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}

pub fn align_down(value: u64, align: u64) -> u64 {
    value & !(align - 1)
}

pub fn guest_range(addr: u64, len: usize, mem_len: usize) -> Option<usize> {
    let end = addr.checked_add(len as u64)?;
    let mappings = GUEST_MAPPINGS.lock().unwrap();
    if let Some(mapping) = mappings
        .iter()
        .rev()
        .find(|mapping| addr >= mapping.virt && addr - mapping.virt < mapping.len)
    {
        let phys = mapping.phys.checked_add(addr - mapping.virt)?;
        let mut map_end = mapping.virt.checked_add(mapping.len)?;
        while map_end < end {
            let mapping = mappings
                .iter()
                .rev()
                .find(|mapping| map_end >= mapping.virt && map_end - mapping.virt < mapping.len)?;
            let next_phys = mapping.phys.checked_add(map_end - mapping.virt)?;
            if next_phys != phys.checked_add(map_end - addr)? {
                return None;
            }
            map_end = mapping.virt.checked_add(mapping.len)?;
        }
        let offset = phys.checked_sub(GUEST_MEM_BASE)? as usize;
        return (offset.checked_add(len)? <= mem_len).then_some(offset);
    }

    let high_end = GUEST_MEM_BASE.checked_add(mem_len as u64)?;
    if addr >= GUEST_MEM_BASE && end <= high_end {
        return Some((addr - GUEST_MEM_BASE) as usize);
    }

    None
}

pub fn translate_guest_addr(addr: u64, len: usize, mem_len: usize) -> Option<usize> {
    guest_range(addr, len, mem_len)
}

pub fn set_guest_mappings(mappings: &[(u64, u64, u64)]) {
    let mut current = GUEST_MAPPINGS.lock().unwrap();
    current.clear();
    current.extend(
        mappings
            .iter()
            .map(|&(virt, phys, len)| GuestMapping { virt, phys, len }),
    );
}

pub fn add_guest_mapping(virt: u64, phys: u64, len: u64) {
    GUEST_MAPPINGS.lock().unwrap().push(GuestMapping { virt, phys, len });
}

pub fn guest_cstr(addr: u64, max_len: usize, memory: &[u8]) -> Option<Vec<u8>> {
    let start = guest_range(addr, 1, memory.len())?;
    let mut bytes = Vec::new();
    for i in 0..max_len {
        if start + i >= memory.len() {
            return None;
        }
        let b = memory[start + i];
        if b == 0 {
            return Some(bytes);
        }
        bytes.push(b);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::handle_write;
    use crate::state::SyscallState;
    use std::fs::{self, File};

    #[test]
    fn syscall_buffers_across_guest_mappings() {
        set_guest_mappings(&[
            (0x1000, GUEST_MEM_BASE, 0x1000),
            (0x2000, GUEST_MEM_BASE + 0x1000, 0x1000),
            (0x3000, GUEST_MEM_BASE + 0x2000, 0x1000),
        ]);
        assert_eq!(guest_range(0x1100, 0x20, 0x3000), Some(0x100));
        assert_eq!(guest_range(0x1ff0, 0x20, 0x3000), Some(0xff0));
        assert_eq!(guest_range(0x1ff0, 0x1020, 0x3000), Some(0xff0));
        assert_eq!(guest_range(0x1ff0, 0, 0x3000), Some(0xff0));
        assert_eq!(guest_range(0x1ff0, 0x2010, 0x3000), Some(0xff0));
        assert_eq!(guest_range(0x1ff0, 0x2011, 0x3000), None);
        assert_eq!(guest_range(0x1ff0, 0x20, 0x1000), None);
        assert_eq!(guest_range(u64::MAX, 2, 0x3000), None);

        let memory: Vec<u8> = (0..0x3000).map(|i| (i % 251) as u8).collect();
        let path = std::env::temp_dir().join(format!("bebop-syscall-cross-mapping-{}", std::process::id()));
        let mut state = SyscallState::new();
        let fd = state.alloc_fd(File::create_new(&path).unwrap());
        assert_eq!(handle_write(&mut state, fd, 0x1ff0, 0x1020, &memory), (0x1020, false));
        drop(state);
        assert_eq!(fs::read(&path).unwrap(), memory[0xff0..0x2010]);
        fs::remove_file(path).unwrap();

        set_guest_mappings(&[
            (0x1000, GUEST_MEM_BASE, 0x1000),
            (0x3000, GUEST_MEM_BASE + 0x2000, 0x1000),
        ]);
        assert_eq!(guest_range(0x1ff0, 0x1020, 0x3000), None);
        add_guest_mapping(0x2000, GUEST_MEM_BASE + 0x2000, 0x1000);
        assert_eq!(guest_range(0x1ff0, 0x20, 0x3000), None);
        add_guest_mapping(0x2000, GUEST_MEM_BASE + 0x1000, 0x1000);
        assert_eq!(guest_range(0x1ff0, 0x1020, 0x3000), Some(0xff0));

        set_guest_mappings(&[(GUEST_MEM_BASE, GUEST_MEM_BASE, 0x1000)]);
        assert_eq!(guest_range(GUEST_MEM_BASE + 0xff0, 0x20, 0x3000), None);
        set_guest_mappings(&[]);
        assert_eq!(guest_range(GUEST_MEM_BASE + 0xff0, 0x20, 0x3000), Some(0xff0));
        assert_eq!(guest_range(GUEST_MEM_BASE + 0x2ff0, 0x20, 0x3000), None);
    }
}
