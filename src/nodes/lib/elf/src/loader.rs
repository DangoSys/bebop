use crate::constants::*;
use crate::reloc::{apply_dynamic_relocations, apply_pointer_fixup, apply_section_relocations, RelocCtx};
use crate::symbols::{read_ifunc_map, read_shdrs};
use crate::types::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub fn load_elf(path: &str, mem_base: &mut [u8], mem_base_addr: u64) -> Result<(u64, Option<TlsInfo>), String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open ELF file: {}", e))?;

    let mut ehdr_bytes = [0u8; std::mem::size_of::<Elf64Ehdr>()];
    file.read_exact(&mut ehdr_bytes)
        .map_err(|e| format!("Failed to read ELF header: {}", e))?;
    // SAFETY: ehdr_bytes is exactly sizeof(Elf64Ehdr) bytes from disk; Elf64Ehdr is
    // #[repr(C)] with only POD fields (u8/u16/u32/u64), so any byte pattern is valid.
    let ehdr: Elf64Ehdr = unsafe { std::ptr::read(ehdr_bytes.as_ptr() as *const _) };

    if ehdr.e_ident[EI_MAG0] != ELFMAG0
        || ehdr.e_ident[EI_MAG1] != ELFMAG1
        || ehdr.e_ident[EI_MAG2] != ELFMAG2
        || ehdr.e_ident[EI_MAG3] != ELFMAG3
    {
        return Err("Not a valid ELF file".to_string());
    }

    // ET_DYN = 3 (shared object / PIE)
    let is_pie = ehdr.e_type == 3;

    let shdrs = if ehdr.e_shoff != 0 && ehdr.e_shnum > 0 {
        Some(read_shdrs(&mut file, &ehdr)?)
    } else {
        None
    };
    let ifunc_map = if let Some(shdrs) = shdrs.as_ref() {
        read_ifunc_map(&mut file, shdrs)?
    } else {
        HashMap::new()
    };

    let (min_vaddr, needs_relocation, mut tls_info, dynamic_phdr, all_phdrs) =
        scan_program_headers(&mut file, &ehdr, mem_base_addr)?;

    load_segments(
        &mut file,
        &all_phdrs,
        mem_base,
        mem_base_addr,
        min_vaddr,
        is_pie,
        needs_relocation,
    )?;

    let entry = compute_entry(ehdr.e_entry, mem_base_addr, min_vaddr, is_pie, needs_relocation);

    let mut ctx = RelocCtx {
        mem_base,
        mem_base_addr,
        min_vaddr,
        is_pie,
        needs_relocation,
    };

    if let Some(shdrs) = shdrs.as_ref() {
        apply_section_relocations(&mut file, shdrs, &ifunc_map, &mut ctx)?;
    }

    if let Some(dyn_phdr) = dynamic_phdr {
        apply_dynamic_relocations(&dyn_phdr, &ifunc_map, &mut ctx)?;
    }

    if let Some(tls) = tls_info.as_mut() {
        if is_pie || needs_relocation {
            tls.vaddr = mem_base_addr + (tls.vaddr - min_vaddr);
        }
    }

    apply_pointer_fixup(&all_phdrs, &mut ctx);

    Ok((entry, tls_info))
}

#[allow(clippy::type_complexity)]
fn scan_program_headers(
    file: &mut File,
    ehdr: &Elf64Ehdr,
    mem_base_addr: u64,
) -> Result<(u64, bool, Option<TlsInfo>, Option<Elf64Phdr>, Vec<Elf64Phdr>), String> {
    let mut min_vaddr = u64::MAX;
    let mut needs_relocation = false;
    let mut tls_info: Option<TlsInfo> = None;
    let mut dynamic_phdr: Option<Elf64Phdr> = None;
    let mut all_phdrs: Vec<Elf64Phdr> = Vec::new();

    file.seek(SeekFrom::Start(ehdr.e_phoff))
        .map_err(|e| format!("Failed to seek to program headers: {}", e))?;

    for _ in 0..ehdr.e_phnum {
        let mut phdr_bytes = [0u8; std::mem::size_of::<Elf64Phdr>()];
        file.read_exact(&mut phdr_bytes)
            .map_err(|e| format!("Failed to read program header: {}", e))?;
        // SAFETY: phdr_bytes is exactly sizeof(Elf64Phdr) bytes from disk; Elf64Phdr is
        // #[repr(C)] with only POD fields, so any byte pattern is valid.
        let phdr: Elf64Phdr = unsafe { std::ptr::read(phdr_bytes.as_ptr() as *const _) };

        all_phdrs.push(phdr);

        if phdr.p_type == PT_LOAD {
            if phdr.p_vaddr < min_vaddr {
                min_vaddr = phdr.p_vaddr;
            }
            if phdr.p_vaddr < mem_base_addr {
                needs_relocation = true;
            }
        }

        if phdr.p_type == PT_TLS {
            tls_info = Some(TlsInfo {
                vaddr: phdr.p_vaddr,
                filesz: phdr.p_filesz,
                memsz: phdr.p_memsz,
                align: phdr.p_align,
            });
        }

        if phdr.p_type == PT_DYNAMIC {
            dynamic_phdr = Some(phdr);
        }
    }

    Ok((min_vaddr, needs_relocation, tls_info, dynamic_phdr, all_phdrs))
}

fn load_segments(
    file: &mut File,
    all_phdrs: &[Elf64Phdr],
    mem_base: &mut [u8],
    mem_base_addr: u64,
    min_vaddr: u64,
    is_pie: bool,
    needs_relocation: bool,
) -> Result<(), String> {
    for phdr in all_phdrs.iter() {
        if phdr.p_type != PT_LOAD {
            continue;
        }
        let addr = if is_pie || needs_relocation {
            mem_base_addr + (phdr.p_vaddr - min_vaddr)
        } else {
            phdr.p_vaddr
        };

        if addr < mem_base_addr || addr + phdr.p_memsz > mem_base_addr + mem_base.len() as u64 {
            continue;
        }
        let offset = (addr - mem_base_addr) as usize;

        file.seek(SeekFrom::Start(phdr.p_offset))
            .map_err(|e| format!("Failed to seek to segment: {}", e))?;
        file.read_exact(&mut mem_base[offset..offset + phdr.p_filesz as usize])
            .map_err(|e| format!("Failed to read segment: {}", e))?;

        if phdr.p_memsz > phdr.p_filesz {
            let bss_start = offset + phdr.p_filesz as usize;
            let bss_end = offset + phdr.p_memsz as usize;
            mem_base[bss_start..bss_end].fill(0);
        }
    }
    Ok(())
}

fn compute_entry(e_entry: u64, mem_base_addr: u64, min_vaddr: u64, is_pie: bool, needs_relocation: bool) -> u64 {
    if e_entry >= 0xffffffff80000000 {
        // Linux kernel entry: 0xffffffff80000000 -> 0x80000000
        e_entry - 0xffffffff80000000 + 0x80000000
    } else if is_pie || needs_relocation {
        mem_base_addr + (e_entry - min_vaddr)
    } else {
        e_entry
    }
}
