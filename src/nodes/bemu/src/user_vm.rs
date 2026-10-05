use crate::process::{align_down, align_up, guest_offset, write_guest, PAGE_SIZE};
use crate::root::memory::Pages;
use bebop_syscall::add_guest_mapping;
use memory::Memory;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
pub(crate) struct GuestMap {
    pub(crate) virt: u64,
    pub(crate) phys: u64,
    pub(crate) len: u64,
}

pub(crate) struct UserVm {
    root: u64,
    pages: Arc<Mutex<Pages>>,
    allocations: Vec<(u64, u64)>,
    pub(crate) maps: Vec<GuestMap>,
}

impl UserVm {
    pub(crate) fn new(memory: &dyn Memory, pages: Arc<Mutex<Pages>>, image: (u64, u64)) -> Result<Self, String> {
        let mut vm = Self {
            root: 0,
            pages,
            allocations: vec![image],
            maps: Vec::new(),
        };
        vm.root = vm.alloc_table(memory)?;
        Ok(vm)
    }

    pub(crate) fn allocate_physical(&mut self, bytes: u64) -> Result<u64, String> {
        let size = align_up(bytes, PAGE_SIZE);
        let address = self.pages.lock().expect("DDR page pool poisoned").allocate(size)?;
        self.allocations.push((address, size));
        Ok(address)
    }

    pub(crate) fn satp(&self) -> u64 {
        (8u64 << 60) | ((self.root >> 12) & 0x0000_0fff_ffff_ffff)
    }

    pub(crate) fn map_range(
        &mut self,
        memory: &dyn Memory,
        virt: u64,
        phys: u64,
        len: u64,
        flags: u64,
    ) -> Result<(), String> {
        if len == 0 {
            return Ok(());
        }
        let virt_start = align_down(virt, PAGE_SIZE);
        let phys_start = align_down(phys, PAGE_SIZE);
        let virt_end = align_up(virt + len, PAGE_SIZE);
        let mut vaddr = virt_start;
        let mut paddr = phys_start;
        while vaddr < virt_end {
            self.map_page(memory, vaddr, paddr, flags)?;
            vaddr += PAGE_SIZE;
            paddr += PAGE_SIZE;
        }
        self.maps.push(GuestMap {
            virt: virt_start,
            phys: phys_start,
            len: virt_end - virt_start,
        });
        add_guest_mapping(virt_start, phys_start, virt_end - virt_start);
        Ok(())
    }

    pub(crate) fn alloc_user_pages(
        &mut self,
        memory: &dyn Memory,
        virt: u64,
        len: u64,
        flags: u64,
    ) -> Result<u64, String> {
        let size = align_up(len, PAGE_SIZE);
        let phys = self.pages.lock().expect("DDR page pool poisoned").allocate(size)?;
        self.allocations.push((phys, size));
        let off = guest_offset(memory, phys)?;
        let end = off + size as usize;
        if end > memory.len() {
            return Err(format!(
                "user physical page allocator exceeds memory: addr=0x{phys:x} size={size}"
            ));
        }
        memory.fill(off, end - off, 0);
        self.map_range(memory, virt, phys, size, flags)?;
        Ok(phys)
    }

    pub(crate) fn free_user_pages(&mut self, memory: &dyn Memory, virt: u64, len: u64) -> Result<(), String> {
        let size = align_up(len, PAGE_SIZE);
        let end = virt + size;
        let ranges: Vec<_> = self
            .maps
            .iter()
            .filter_map(|map| {
                let start = virt.max(map.virt);
                let stop = end.min(map.virt + map.len);
                (start < stop).then(|| (start, map.phys + start - map.virt, stop - start))
            })
            .collect();
        for &(start, _, bytes) in &ranges {
            for address in (start..start + bytes).step_by(PAGE_SIZE as usize) {
                let vpn = [
                    (address >> 12) & 0x1ff,
                    (address >> 21) & 0x1ff,
                    (address >> 30) & 0x1ff,
                ];
                let l2 = (self.read_pte(memory, self.root, vpn[2])? >> 10) << 12;
                let l1 = (self.read_pte(memory, l2, vpn[1])? >> 10) << 12;
                self.write_pte(memory, l1, vpn[0], 0)?;
            }
        }
        let mut retained = Vec::new();
        for map in self.maps.drain(..) {
            if map.virt >= virt + size || map.virt + map.len <= virt {
                retained.push(map);
                continue;
            }
            if map.virt < virt {
                retained.push(GuestMap {
                    len: virt - map.virt,
                    ..map
                });
            }
            if map.virt + map.len > virt + size {
                let offset = virt + size - map.virt;
                retained.push(GuestMap {
                    virt: virt + size,
                    phys: map.phys + offset,
                    len: map.len - offset,
                });
            }
        }
        self.maps = retained;
        for (_, phys, size) in ranges {
            let mut owned = Vec::new();
            for (address, bytes) in self.allocations.drain(..) {
                if address >= phys + size || address + bytes <= phys {
                    owned.push((address, bytes));
                    continue;
                }
                if address < phys {
                    owned.push((address, phys - address));
                }
                if address + bytes > phys + size {
                    owned.push((phys + size, address + bytes - phys - size));
                }
            }
            self.allocations = owned;
            self.pages.lock().expect("DDR page pool poisoned").release(phys, size);
        }
        Ok(())
    }

    pub(crate) fn write_user(&self, memory: &dyn Memory, virt: u64, bytes: &[u8]) -> Result<(), String> {
        let phys = self
            .virt_to_phys(virt, bytes.len() as u64)
            .ok_or_else(|| format!("user write to unmapped VA: addr=0x{virt:x} size={}", bytes.len()))?;
        write_guest(memory, phys, bytes)
    }

    pub(crate) fn virt_to_phys(&self, virt: u64, len: u64) -> Option<u64> {
        let end = virt.checked_add(len)?;
        for map in self.maps.iter().rev() {
            let map_end = map.virt.checked_add(map.len)?;
            if virt >= map.virt && end <= map_end {
                return map.phys.checked_add(virt - map.virt);
            }
        }
        None
    }

    pub(crate) fn map_page(&mut self, memory: &dyn Memory, virt: u64, phys: u64, flags: u64) -> Result<(), String> {
        let vpn = [(virt >> 12) & 0x1ff, (virt >> 21) & 0x1ff, (virt >> 30) & 0x1ff];
        let l2 = self.ensure_table(memory, self.root, vpn[2])?;
        let l1 = self.ensure_table(memory, l2, vpn[1])?;
        let leaf = ((phys >> 12) << 10) | flags | 0x1 | 0x10 | 0x40 | 0x80;
        self.write_pte(memory, l1, vpn[0], leaf)
    }

    pub(crate) fn ensure_table(&mut self, memory: &dyn Memory, table: u64, idx: u64) -> Result<u64, String> {
        let pte = self.read_pte(memory, table, idx)?;
        if pte & 0x1 != 0 {
            return Ok(((pte >> 10) << 12) & !0xfffu64);
        }
        let child = self.alloc_table(memory)?;
        self.write_pte(memory, table, idx, ((child >> 12) << 10) | 0x1)?;
        Ok(child)
    }

    pub(crate) fn alloc_table(&mut self, memory: &dyn Memory) -> Result<u64, String> {
        let table = self.pages.lock().expect("DDR page pool poisoned").allocate(PAGE_SIZE)?;
        self.allocations.push((table, PAGE_SIZE));
        let off = guest_offset(memory, table)?;
        memory.fill(off, PAGE_SIZE as usize, 0);
        Ok(table)
    }

    pub(crate) fn read_pte(&self, memory: &dyn Memory, table: u64, idx: u64) -> Result<u64, String> {
        let off = guest_offset(memory, table + idx * 8)?;
        let mut bytes = [0u8; 8];
        memory.read_buffer(off, &mut bytes);
        Ok(u64::from_le_bytes(bytes))
    }

    pub(crate) fn write_pte(&self, memory: &dyn Memory, table: u64, idx: u64, value: u64) -> Result<(), String> {
        write_guest(memory, table + idx * 8, &value.to_le_bytes())
    }
}

impl Drop for UserVm {
    fn drop(&mut self) {
        let mut pages = self.pages.lock().expect("DDR page pool poisoned");
        for (address, bytes) in self.allocations.drain(..) {
            pages.release(address, bytes);
        }
    }
}
