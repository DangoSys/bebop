use crate::constants::*;
use crate::reloc::{apply_dynamic_relocations, apply_pointer_fixup, apply_section_relocations, RelocCtx};
use crate::symbols::{read_ifunc_map, read_shdrs};
use crate::types::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub fn load_elf(path: &str, mem_base: &mut [u8], mem_base_addr: u64) -> Result<LoadInfo, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open ELF file: {}", e))?;

    let ehdr = read_elf_header(&mut file)?;
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

    let scan = scan_program_headers(&mut file, &ehdr, mem_base_addr)?;

    load_segments(
        &mut file,
        &scan.all_phdrs,
        mem_base,
        mem_base_addr,
        scan.min_vaddr,
        is_pie,
        scan.needs_relocation,
    )?;

    let entry = compute_entry(
        ehdr.e_entry,
        mem_base_addr,
        scan.min_vaddr,
        is_pie,
        scan.needs_relocation,
    );

    let mut ctx = RelocCtx {
        mem_base,
        mem_base_addr,
        min_vaddr: scan.min_vaddr,
        is_pie,
        needs_relocation: scan.needs_relocation,
    };

    if let Some(shdrs) = shdrs.as_ref() {
        apply_section_relocations(&mut file, shdrs, &ifunc_map, &mut ctx)?;
    }

    if let Some(dyn_phdr) = scan.dynamic_phdr {
        apply_dynamic_relocations(&dyn_phdr, &ifunc_map, &mut ctx)?;
    }

    let mut tls_info = scan.tls;
    if let Some(tls) = tls_info.as_mut() {
        if is_pie || scan.needs_relocation {
            tls.vaddr = mem_base_addr + (tls.vaddr - scan.min_vaddr);
        }
    }

    apply_pointer_fixup(&scan.all_phdrs, &mut ctx);
    let analysis = ElfAnalysis {
        original_entry: ehdr.e_entry,
        entry,
        min_vaddr: scan.min_vaddr,
        max_vaddr: scan.max_vaddr,
        image_end: scan.image_end,
        is_pie,
        needs_relocation: scan.needs_relocation,
        load_segments: scan
            .all_phdrs
            .iter()
            .filter(|phdr| phdr.p_type == PT_LOAD)
            .map(|phdr| ElfLoadSegment {
                vaddr: phdr.p_vaddr,
                memsz: phdr.p_memsz,
                filesz: phdr.p_filesz,
                flags: phdr.p_flags,
            })
            .collect(),
    };

    Ok(LoadInfo {
        entry,
        image_end: scan.image_end,
        tls: tls_info,
        program_headers: program_header_info(
            &ehdr,
            &scan.all_phdrs,
            mem_base_addr,
            scan.min_vaddr,
            is_pie,
            scan.needs_relocation,
        ),
        analysis,
    })
}

pub fn analyze_elf(path: &str, mem_base_addr: u64) -> Result<ElfAnalysis, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open ELF file: {}", e))?;
    let ehdr = read_elf_header(&mut file)?;
    let is_pie = ehdr.e_type == 3;
    let scan = scan_program_headers(&mut file, &ehdr, mem_base_addr)?;
    let entry = compute_entry(
        ehdr.e_entry,
        mem_base_addr,
        scan.min_vaddr,
        is_pie,
        scan.needs_relocation,
    );

    Ok(ElfAnalysis {
        original_entry: ehdr.e_entry,
        entry,
        min_vaddr: scan.min_vaddr,
        max_vaddr: scan.max_vaddr,
        image_end: scan.image_end,
        is_pie,
        needs_relocation: scan.needs_relocation,
        load_segments: scan
            .all_phdrs
            .iter()
            .filter(|phdr| phdr.p_type == PT_LOAD)
            .map(|phdr| ElfLoadSegment {
                vaddr: phdr.p_vaddr,
                memsz: phdr.p_memsz,
                filesz: phdr.p_filesz,
                flags: phdr.p_flags,
            })
            .collect(),
    })
}

struct ProgramHeaderScan {
    min_vaddr: u64,
    max_vaddr: u64,
    image_end: u64,
    needs_relocation: bool,
    tls: Option<TlsInfo>,
    dynamic_phdr: Option<Elf64Phdr>,
    all_phdrs: Vec<Elf64Phdr>,
}

fn read_elf_header(file: &mut File) -> Result<Elf64Ehdr, String> {
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("Failed to seek to ELF header: {}", e))?;
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

    Ok(ehdr)
}

fn scan_program_headers(file: &mut File, ehdr: &Elf64Ehdr, mem_base_addr: u64) -> Result<ProgramHeaderScan, String> {
    let mut min_vaddr = u64::MAX;
    let mut max_vaddr = 0;
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
            let seg_end = phdr
                .p_vaddr
                .checked_add(phdr.p_memsz)
                .ok_or_else(|| "ELF PT_LOAD address range overflows".to_string())?;
            if seg_end > max_vaddr {
                max_vaddr = seg_end;
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

    let image_end = if needs_relocation {
        mem_base_addr + (max_vaddr - min_vaddr)
    } else {
        max_vaddr
    };

    Ok(ProgramHeaderScan {
        min_vaddr,
        max_vaddr,
        image_end,
        needs_relocation,
        tls: tls_info,
        dynamic_phdr,
        all_phdrs,
    })
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

fn program_header_info(
    ehdr: &Elf64Ehdr,
    all_phdrs: &[Elf64Phdr],
    mem_base_addr: u64,
    min_vaddr: u64,
    is_pie: bool,
    needs_relocation: bool,
) -> ProgramHeaderInfo {
    let addr = all_phdrs
        .iter()
        .find_map(|phdr| {
            let file_start = phdr.p_offset;
            let file_end = phdr.p_offset.checked_add(phdr.p_filesz)?;
            if phdr.p_type != PT_LOAD || ehdr.e_phoff < file_start || ehdr.e_phoff >= file_end {
                return None;
            }

            let loaded_addr = if is_pie || needs_relocation {
                mem_base_addr + (phdr.p_vaddr - min_vaddr)
            } else {
                phdr.p_vaddr
            };
            Some(loaded_addr + (ehdr.e_phoff - phdr.p_offset))
        })
        .unwrap_or(0);

    ProgramHeaderInfo {
        addr,
        entry_size: if ehdr.e_phentsize == 0 {
            std::mem::size_of::<Elf64Phdr>() as u64
        } else {
            ehdr.e_phentsize as u64
        },
        count: ehdr.e_phnum as u64,
    }
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
