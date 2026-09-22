use super::spike::{align_down, align_up, guest_offset, write_guest};
use super::*;

#[derive(Clone, Copy)]
pub(super) struct GuestMap {
    pub(super) virt: u64,
    pub(super) phys: u64,
    pub(super) len: u64,
}

pub(super) struct PkVm {
    root: u64,
    next_pt: u64,
    pt_end: u64,
    next_page: u64,
    page_end: u64,
    free_pages: Vec<(u64, u64)>,
    pub(super) maps: Vec<GuestMap>,
}

impl PkVm {
    pub(super) fn new(
        memory: &mut [u8],
        root: u64,
        pt_end: u64,
        next_page: u64,
        page_end: u64,
    ) -> Result<Self, String> {
        let mut vm = Self {
            root,
            next_pt: root,
            pt_end,
            next_page,
            page_end,
            free_pages: Vec::new(),
            maps: Vec::new(),
        };
        vm.alloc_table(memory)?;
        Ok(vm)
    }

    pub(super) fn satp(&self) -> u64 {
        (8u64 << 60) | ((self.root >> 12) & 0x0000_0fff_ffff_ffff)
    }

    pub(super) fn map_range(
        &mut self,
        memory: &mut [u8],
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

    pub(super) fn alloc_user_pages(
        &mut self,
        memory: &mut [u8],
        virt: u64,
        len: u64,
        flags: u64,
    ) -> Result<u64, String> {
        let size = align_up(len, PAGE_SIZE);
        let phys = match self.take_free_pages(size) {
            Some(phys) => phys,
            None => {
                let phys = self.next_page;
                self.next_page = self
                    .next_page
                    .checked_add(size)
                    .ok_or_else(|| "pk physical page allocator overflow".to_string())?;
                if self.next_page > self.page_end {
                    return Err("pk user page reserve exhausted".to_string());
                }
                phys
            }
        };
        let off = guest_offset(memory, phys)?;
        let end = off + size as usize;
        if end > memory.len() {
            return Err(format!(
                "pk physical page allocator exceeds memory: addr=0x{phys:x} size={size}"
            ));
        }
        memory[off..end].fill(0);
        self.map_range(memory, virt, phys, size, flags)?;
        Ok(phys)
    }

    pub(super) fn free_user_pages(&mut self, virt: u64, len: u64) -> Result<(), String> {
        let size = align_up(len, PAGE_SIZE);
        let phys = self
            .virt_to_phys(virt, size)
            .ok_or_else(|| "pk munmap range is not mapped".to_string())?;
        self.free_pages.push((phys, size));
        Ok(())
    }

    pub(super) fn take_free_pages(&mut self, size: u64) -> Option<u64> {
        let index = self.free_pages.iter().position(|(_, len)| *len >= size)?;
        let (phys, len) = self.free_pages[index];
        if len == size {
            self.free_pages.swap_remove(index);
        } else {
            self.free_pages[index] = (phys + size, len - size);
        }
        Some(phys)
    }

    pub(super) fn write_user(&self, memory: &mut [u8], virt: u64, bytes: &[u8]) -> Result<(), String> {
        let phys = self
            .virt_to_phys(virt, bytes.len() as u64)
            .ok_or_else(|| format!("user write to unmapped VA: addr=0x{virt:x} size={}", bytes.len()))?;
        write_guest(memory, phys, bytes)
    }

    pub(super) fn virt_to_phys(&self, virt: u64, len: u64) -> Option<u64> {
        let end = virt.checked_add(len)?;
        for map in self.maps.iter().rev() {
            let map_end = map.virt.checked_add(map.len)?;
            if virt >= map.virt && end <= map_end {
                return map.phys.checked_add(virt - map.virt);
            }
        }
        None
    }

    pub(super) fn map_page(&mut self, memory: &mut [u8], virt: u64, phys: u64, flags: u64) -> Result<(), String> {
        let vpn = [(virt >> 12) & 0x1ff, (virt >> 21) & 0x1ff, (virt >> 30) & 0x1ff];
        let l2 = self.ensure_table(memory, self.root, vpn[2])?;
        let l1 = self.ensure_table(memory, l2, vpn[1])?;
        let leaf = ((phys >> 12) << 10) | flags | 0x1 | 0x10 | 0x40 | 0x80;
        self.write_pte(memory, l1, vpn[0], leaf)
    }

    pub(super) fn ensure_table(&mut self, memory: &mut [u8], table: u64, idx: u64) -> Result<u64, String> {
        let pte = self.read_pte(memory, table, idx)?;
        if pte & 0x1 != 0 {
            return Ok(((pte >> 10) << 12) & !0xfffu64);
        }
        let child = self.alloc_table(memory)?;
        self.write_pte(memory, table, idx, ((child >> 12) << 10) | 0x1)?;
        Ok(child)
    }

    pub(super) fn alloc_table(&mut self, memory: &mut [u8]) -> Result<u64, String> {
        let table = self.next_pt;
        self.next_pt = self
            .next_pt
            .checked_add(PAGE_SIZE)
            .ok_or_else(|| "pk page table allocator overflow".to_string())?;
        if self.next_pt > self.pt_end {
            return Err("pk page table reserve exhausted".to_string());
        }
        let off = guest_offset(memory, table)?;
        memory[off..off + PAGE_SIZE as usize].fill(0);
        Ok(table)
    }

    pub(super) fn read_pte(&self, memory: &[u8], table: u64, idx: u64) -> Result<u64, String> {
        let off = guest_offset(memory, table + idx * 8)?;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&memory[off..off + 8]);
        Ok(u64::from_le_bytes(bytes))
    }

    pub(super) fn write_pte(&self, memory: &mut [u8], table: u64, idx: u64, value: u64) -> Result<(), String> {
        write_guest(memory, table + idx * 8, &value.to_le_bytes())
    }
}
