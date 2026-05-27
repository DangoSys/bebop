use crate::constants::*;
use crate::types::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub struct RelocCtx<'a> {
    pub mem_base: &'a mut [u8],
    pub mem_base_addr: u64,
    pub min_vaddr: u64,
    pub is_pie: bool,
    pub needs_relocation: bool,
}

impl RelocCtx<'_> {
    fn relocate_addr(&self, vaddr: u64) -> u64 {
        if self.is_pie || self.needs_relocation {
            self.mem_base_addr + (vaddr - self.min_vaddr)
        } else {
            vaddr
        }
    }

    fn write_resolved(&mut self, target_vaddr: u64, resolved_vaddr: u64) {
        let target_addr = self.relocate_addr(target_vaddr);
        let resolved_addr = self.relocate_addr(resolved_vaddr);

        if target_addr >= self.mem_base_addr && target_addr + 8 <= self.mem_base_addr + self.mem_base.len() as u64 {
            let target_offset = (target_addr - self.mem_base_addr) as usize;
            self.mem_base[target_offset..target_offset + 8].copy_from_slice(&resolved_addr.to_le_bytes());
        }
    }
}

fn apply_irelative(ctx: &mut RelocCtx, rela: &Elf64Rela, ifunc_map: &HashMap<u64, u64>) -> Result<(), String> {
    let r_type = (rela.r_info & 0xffffffff) as u32;
    if r_type != R_RISCV_IRELATIVE {
        return Ok(());
    }
    let resolver_vaddr = rela.r_addend as u64;
    let resolved_vaddr = *ifunc_map
        .get(&resolver_vaddr)
        .ok_or_else(|| format!("Unsupported IFUNC resolver: {:#x}", resolver_vaddr))?;
    ctx.write_resolved(rela.r_offset, resolved_vaddr);
    Ok(())
}

pub fn apply_section_relocations(
    file: &mut File,
    shdrs: &[Elf64Shdr],
    ifunc_map: &HashMap<u64, u64>,
    ctx: &mut RelocCtx,
) -> Result<(), String> {
    for shdr in shdrs.iter() {
        if shdr.sh_type != SHT_RELA || shdr.sh_size == 0 {
            continue;
        }
        file.seek(SeekFrom::Start(shdr.sh_offset))
            .map_err(|e| format!("Failed to seek to RELA section: {}", e))?;

        let rela_count = shdr.sh_size / std::mem::size_of::<Elf64Rela>() as u64;
        for _ in 0..rela_count {
            let mut rela_bytes = [0u8; std::mem::size_of::<Elf64Rela>()];
            file.read_exact(&mut rela_bytes)
                .map_err(|e| format!("Failed to read RELA entry: {}", e))?;
            // SAFETY: rela_bytes is exactly sizeof(Elf64Rela) bytes from disk; Elf64Rela is
            // #[repr(C)] with only POD fields, so any byte pattern is valid.
            let rela: Elf64Rela = unsafe { std::ptr::read(rela_bytes.as_ptr() as *const _) };
            apply_irelative(ctx, &rela, ifunc_map)?;
        }
    }
    Ok(())
}

pub fn apply_dynamic_relocations(
    dyn_phdr: &Elf64Phdr,
    ifunc_map: &HashMap<u64, u64>,
    ctx: &mut RelocCtx,
) -> Result<(), String> {
    let dyn_addr = ctx.relocate_addr(dyn_phdr.p_vaddr);
    if dyn_addr < ctx.mem_base_addr || dyn_addr + dyn_phdr.p_memsz > ctx.mem_base_addr + ctx.mem_base.len() as u64 {
        return Ok(());
    }

    let dyn_offset = (dyn_addr - ctx.mem_base_addr) as usize;
    let mut rela_addr: Option<u64> = None;
    let mut rela_size: Option<u64> = None;
    let mut rela_ent: Option<u64> = None;

    let dyn_count = dyn_phdr.p_memsz / std::mem::size_of::<Elf64Dyn>() as u64;
    for i in 0..dyn_count {
        let dyn_entry_offset = dyn_offset + (i as usize * std::mem::size_of::<Elf64Dyn>());
        if dyn_entry_offset + std::mem::size_of::<Elf64Dyn>() > ctx.mem_base.len() {
            break;
        }

        // SAFETY: bounds checked above; mem_base is loaded ELF memory; Elf64Dyn is #[repr(C)]
        // with only POD fields, so any byte pattern is valid.
        let dyn_entry: Elf64Dyn = unsafe { std::ptr::read(ctx.mem_base[dyn_entry_offset..].as_ptr() as *const _) };

        match dyn_entry.d_tag {
            DT_RELA => rela_addr = Some(dyn_entry.d_val),
            DT_RELASZ => rela_size = Some(dyn_entry.d_val),
            DT_RELAENT => rela_ent = Some(dyn_entry.d_val),
            _ => {}
        }
    }

    let (Some(rela_vaddr), Some(size), Some(_ent)) = (rela_addr, rela_size, rela_ent) else {
        return Ok(());
    };

    let rela_addr = ctx.relocate_addr(rela_vaddr);
    if rela_addr < ctx.mem_base_addr || rela_addr + size > ctx.mem_base_addr + ctx.mem_base.len() as u64 {
        return Ok(());
    }

    let rela_offset = (rela_addr - ctx.mem_base_addr) as usize;
    let rela_count = size / std::mem::size_of::<Elf64Rela>() as u64;

    for i in 0..rela_count {
        let rela_entry_offset = rela_offset + (i as usize * std::mem::size_of::<Elf64Rela>());
        if rela_entry_offset + std::mem::size_of::<Elf64Rela>() > ctx.mem_base.len() {
            break;
        }

        // SAFETY: bounds checked above; mem_base is loaded ELF memory; Elf64Rela is #[repr(C)]
        // with only POD fields, so any byte pattern is valid.
        let rela: Elf64Rela = unsafe { std::ptr::read(ctx.mem_base[rela_entry_offset..].as_ptr() as *const _) };
        apply_irelative(ctx, &rela, ifunc_map)?;
    }

    Ok(())
}

pub fn apply_pointer_fixup(all_phdrs: &[Elf64Phdr], ctx: &mut RelocCtx) {
    let _ = (all_phdrs, ctx);
}
