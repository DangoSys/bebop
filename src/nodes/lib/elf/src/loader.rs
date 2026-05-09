use crate::constants::*;
use crate::types::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

fn read_shdrs(file: &mut File, ehdr: &Elf64Ehdr) -> Result<Vec<Elf64Shdr>, String> {
    file.seek(SeekFrom::Start(ehdr.e_shoff))
        .map_err(|e| format!("Failed to seek to section headers: {}", e))?;
    let mut shdrs = Vec::with_capacity(ehdr.e_shnum as usize);
    for _ in 0..ehdr.e_shnum {
        let mut shdr_bytes = [0u8; std::mem::size_of::<Elf64Shdr>()];
        file.read_exact(&mut shdr_bytes)
            .map_err(|e| format!("Failed to read section header: {}", e))?;
        let shdr: Elf64Shdr = unsafe { std::ptr::read(shdr_bytes.as_ptr() as *const _) };
        shdrs.push(shdr);
    }
    Ok(shdrs)
}

fn read_ifunc_map(file: &mut File, shdrs: &[Elf64Shdr]) -> Result<HashMap<u64, u64>, String> {
    let mut memcpy_ifunc: Option<u64> = None;
    let mut memcpy_generic: Option<u64> = None;

    for shdr in shdrs.iter() {
        if shdr.sh_type != SHT_SYMTAB {
            continue;
        }
        if shdr.sh_entsize != std::mem::size_of::<Elf64Sym>() as u64 {
            continue;
        }
        let str_idx = shdr.sh_link as usize;
        if str_idx >= shdrs.len() {
            continue;
        }
        let str_shdr = &shdrs[str_idx];
        if str_shdr.sh_size == 0 {
            continue;
        }

        let mut strtab = vec![0u8; str_shdr.sh_size as usize];
        file.seek(SeekFrom::Start(str_shdr.sh_offset))
            .map_err(|e| format!("Failed to seek strtab: {}", e))?;
        file.read_exact(&mut strtab)
            .map_err(|e| format!("Failed to read strtab: {}", e))?;

        if shdr.sh_size == 0 {
            continue;
        }
        let sym_cnt = shdr.sh_size / shdr.sh_entsize;
        file.seek(SeekFrom::Start(shdr.sh_offset))
            .map_err(|e| format!("Failed to seek symtab: {}", e))?;

        for _ in 0..sym_cnt {
            let mut sym_bytes = [0u8; std::mem::size_of::<Elf64Sym>()];
            file.read_exact(&mut sym_bytes)
                .map_err(|e| format!("Failed to read symbol: {}", e))?;
            let sym: Elf64Sym = unsafe { std::ptr::read(sym_bytes.as_ptr() as *const _) };
            if sym.st_name as usize >= strtab.len() {
                continue;
            }

            let name_start = sym.st_name as usize;
            let name_end = strtab[name_start..]
                .iter()
                .position(|&b| b == 0)
                .map(|i| name_start + i)
                .unwrap_or(strtab.len());
            let name = std::str::from_utf8(&strtab[name_start..name_end]).unwrap_or("");

            if name == "__libc_memcpy_ifunc" {
                memcpy_ifunc = Some(sym.st_value);
            } else if name == "__memcpy_generic" {
                memcpy_generic = Some(sym.st_value);
            }
        }
    }

    let mut map = HashMap::new();
    if let (Some(k), Some(v)) = (memcpy_ifunc, memcpy_generic) {
        map.insert(k, v);
    }
    Ok(map)
}

/// TLS information from ELF
#[derive(Debug, Clone, Copy)]
pub struct TlsInfo {
    pub vaddr: u64,
    pub filesz: u64,
    pub memsz: u64,
    pub align: u64,
}

/// Load ELF file into memory
/// Returns (entry point address, optional TLS info)
pub fn load_elf(path: &str, mem_base: &mut [u8], mem_base_addr: u64) -> Result<(u64, Option<TlsInfo>), String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open ELF file: {}", e))?;

    // Read ELF header
    let mut ehdr_bytes = [0u8; std::mem::size_of::<Elf64Ehdr>()];
    file.read_exact(&mut ehdr_bytes)
        .map_err(|e| format!("Failed to read ELF header: {}", e))?;
    let ehdr: Elf64Ehdr = unsafe { std::ptr::read(ehdr_bytes.as_ptr() as *const _) };

    // Verify ELF magic
    if ehdr.e_ident[EI_MAG0] != ELFMAG0
        || ehdr.e_ident[EI_MAG1] != ELFMAG1
        || ehdr.e_ident[EI_MAG2] != ELFMAG2
        || ehdr.e_ident[EI_MAG3] != ELFMAG3
    {
        return Err("Not a valid ELF file".to_string());
    }

    // Check if this is a PIE (Position Independent Executable)
    // ET_DYN = 3 (shared object / PIE)
    let is_pie = ehdr.e_type == 3;

    // For PIE or EXEC with low addresses, we need to find the lowest vaddr to calculate the load base
    let mut min_vaddr = u64::MAX;
    let mut needs_relocation = false;
    let mut tls_info: Option<TlsInfo> = None;
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

    // First pass: find minimum vaddr, TLS segment, and dynamic segment
    // Also save all program headers for second pass
    let mut dynamic_phdr: Option<Elf64Phdr> = None;
    let mut all_phdrs: Vec<Elf64Phdr> = Vec::new();
    file.seek(SeekFrom::Start(ehdr.e_phoff))
        .map_err(|e| format!("Failed to seek to program headers: {}", e))?;

    for _ in 0..ehdr.e_phnum {
        let mut phdr_bytes = [0u8; std::mem::size_of::<Elf64Phdr>()];
        file.read_exact(&mut phdr_bytes)
            .map_err(|e| format!("Failed to read program header: {}", e))?;
        let phdr: Elf64Phdr = unsafe { std::ptr::read(phdr_bytes.as_ptr() as *const _) };

        // Save for second pass
        all_phdrs.push(phdr);

        if phdr.p_type == PT_LOAD {
            let addr = phdr.p_vaddr;
            if addr < min_vaddr {
                min_vaddr = addr;
            }
            // Check if this segment needs relocation (address < DRAM_BASE)
            if addr < mem_base_addr {
                needs_relocation = true;
            }
        }

        // Parse TLS segment
        if phdr.p_type == PT_TLS {
            tls_info = Some(TlsInfo {
                vaddr: phdr.p_vaddr,
                filesz: phdr.p_filesz,
                memsz: phdr.p_memsz,
                align: phdr.p_align,
            });
        }

        // Save dynamic segment for IFUNC resolution
        if phdr.p_type == PT_DYNAMIC {
            dynamic_phdr = Some(phdr);
        }
    }

    // Second pass: load segments using saved program headers
    for (_i, phdr) in all_phdrs.iter().enumerate() {
        if phdr.p_type == PT_LOAD {
            // Determine the load address:
            // - For PIE or EXEC with low addresses: relocate to mem_base_addr
            // - For non-PIE with high addresses: use paddr directly
            let addr = if is_pie || needs_relocation {
                let base_addr = phdr.p_vaddr;
                mem_base_addr + (base_addr - min_vaddr)
            } else {
                phdr.p_vaddr
            };

            if addr >= mem_base_addr && addr + phdr.p_memsz <= mem_base_addr + mem_base.len() as u64 {
                let offset = (addr - mem_base_addr) as usize;

                // Read segment data
                file.seek(SeekFrom::Start(phdr.p_offset))
                    .map_err(|e| format!("Failed to seek to segment: {}", e))?;
                file.read_exact(&mut mem_base[offset..offset + phdr.p_filesz as usize])
                    .map_err(|e| format!("Failed to read segment: {}", e))?;

                // Zero out BSS
                if phdr.p_memsz > phdr.p_filesz {
                    let bss_start = offset + phdr.p_filesz as usize;
                    let bss_end = offset + phdr.p_memsz as usize;
                    mem_base[bss_start..bss_end].fill(0);
                }
            }
        }
    }

    // Calculate entry point
    let entry = if ehdr.e_entry >= 0xffffffff80000000 {
        // Linux kernel entry: 0xffffffff80000000 -> 0x80000000
        ehdr.e_entry - 0xffffffff80000000 + 0x80000000
    } else if is_pie || needs_relocation {
        // PIE or relocated EXEC: add load base to entry point
        mem_base_addr + (ehdr.e_entry - min_vaddr)
    } else {
        // Non-PIE with high addresses: use entry point directly
        ehdr.e_entry
    };

    // Process IFUNC relocations from section headers (for static binaries)
    // IMPORTANT: Do this AFTER loading all segments, so file pointer changes don't affect segment loading
    // Static binaries don't have PT_DYNAMIC, but may have .rela.plt section with IFUNC relocations
    if let Some(shdrs) = shdrs.as_ref() {
        for shdr in shdrs.iter() {
            // Look for RELA sections (SHT_RELA = 4)
            if shdr.sh_type == SHT_RELA && shdr.sh_size > 0 {
                // Read the relocation entries
                file.seek(SeekFrom::Start(shdr.sh_offset))
                    .map_err(|e| format!("Failed to seek to RELA section: {}", e))?;

                let rela_count = shdr.sh_size / std::mem::size_of::<Elf64Rela>() as u64;
                for _ in 0..rela_count {
                    let mut rela_bytes = [0u8; std::mem::size_of::<Elf64Rela>()];
                    file.read_exact(&mut rela_bytes)
                        .map_err(|e| format!("Failed to read RELA entry: {}", e))?;
                    let rela: Elf64Rela = unsafe { std::ptr::read(rela_bytes.as_ptr() as *const _) };

                    let r_type = (rela.r_info & 0xffffffff) as u32;

                    if r_type == R_RISCV_IRELATIVE {
                        let resolver_vaddr = rela.r_addend as u64;
                        let resolved_vaddr = if let Some(v) = ifunc_map.get(&resolver_vaddr) {
                            *v
                        } else {
                            return Err(format!("Unsupported IFUNC resolver: {:#x}", resolver_vaddr));
                        };

                        // Calculate relocation target address
                        let target_addr = if is_pie || needs_relocation {
                            mem_base_addr + (rela.r_offset - min_vaddr)
                        } else {
                            rela.r_offset
                        };

                        let resolved_addr = if is_pie || needs_relocation {
                            mem_base_addr + (resolved_vaddr - min_vaddr)
                        } else {
                            resolved_vaddr
                        };

                        // Write the actual function address to the target
                        if target_addr >= mem_base_addr && target_addr + 8 <= mem_base_addr + mem_base.len() as u64 {
                            let target_offset = (target_addr - mem_base_addr) as usize;
                            mem_base[target_offset..target_offset + 8].copy_from_slice(&resolved_addr.to_le_bytes());
                        }
                    }
                }
            }
        }
    }

    // Process IFUNC relocations if dynamic segment exists (for dynamic binaries)
    if let Some(dyn_phdr) = dynamic_phdr {
        // Calculate dynamic segment address
        let dyn_addr = if is_pie || needs_relocation {
            mem_base_addr + (dyn_phdr.p_vaddr - min_vaddr)
        } else {
            dyn_phdr.p_vaddr
        };

        if dyn_addr >= mem_base_addr && dyn_addr + dyn_phdr.p_memsz <= mem_base_addr + mem_base.len() as u64 {
            let dyn_offset = (dyn_addr - mem_base_addr) as usize;

            // Parse dynamic section to find RELA table
            let mut rela_addr: Option<u64> = None;
            let mut rela_size: Option<u64> = None;
            let mut rela_ent: Option<u64> = None;

            let dyn_count = dyn_phdr.p_memsz / std::mem::size_of::<Elf64Dyn>() as u64;
            for i in 0..dyn_count {
                let dyn_entry_offset = dyn_offset + (i as usize * std::mem::size_of::<Elf64Dyn>());
                if dyn_entry_offset + std::mem::size_of::<Elf64Dyn>() > mem_base.len() {
                    break;
                }

                let dyn_entry: Elf64Dyn = unsafe { std::ptr::read(mem_base[dyn_entry_offset..].as_ptr() as *const _) };

                match dyn_entry.d_tag {
                    DT_RELA => rela_addr = Some(dyn_entry.d_val),
                    DT_RELASZ => rela_size = Some(dyn_entry.d_val),
                    DT_RELAENT => rela_ent = Some(dyn_entry.d_val),
                    _ => {}
                }
            }

            // Process RELA relocations
            if let (Some(rela_vaddr), Some(size), Some(_ent)) = (rela_addr, rela_size, rela_ent) {
                let rela_addr = if is_pie || needs_relocation {
                    mem_base_addr + (rela_vaddr - min_vaddr)
                } else {
                    rela_vaddr
                };

                if rela_addr >= mem_base_addr && rela_addr + size <= mem_base_addr + mem_base.len() as u64 {
                    let rela_offset = (rela_addr - mem_base_addr) as usize;
                    let rela_count = size / std::mem::size_of::<Elf64Rela>() as u64;

                    for i in 0..rela_count {
                        let rela_entry_offset = rela_offset + (i as usize * std::mem::size_of::<Elf64Rela>());
                        if rela_entry_offset + std::mem::size_of::<Elf64Rela>() > mem_base.len() {
                            break;
                        }

                        let rela: Elf64Rela =
                            unsafe { std::ptr::read(mem_base[rela_entry_offset..].as_ptr() as *const _) };

                        let r_type = (rela.r_info & 0xffffffff) as u32;

                        if r_type == R_RISCV_IRELATIVE {
                            let resolver_vaddr = rela.r_addend as u64;
                            let resolved_vaddr = if let Some(v) = ifunc_map.get(&resolver_vaddr) {
                                *v
                            } else {
                                return Err(format!("Unsupported IFUNC resolver: {:#x}", resolver_vaddr));
                            };

                            // Calculate relocation target address
                            let target_addr = if is_pie || needs_relocation {
                                mem_base_addr + (rela.r_offset - min_vaddr)
                            } else {
                                rela.r_offset
                            };

                            let resolved_addr = if is_pie || needs_relocation {
                                mem_base_addr + (resolved_vaddr - min_vaddr)
                            } else {
                                resolved_vaddr
                            };

                            if target_addr >= mem_base_addr && target_addr + 8 <= mem_base_addr + mem_base.len() as u64
                            {
                                let target_offset = (target_addr - mem_base_addr) as usize;
                                mem_base[target_offset..target_offset + 8]
                                    .copy_from_slice(&resolved_addr.to_le_bytes());
                            }
                        }
                    }
                }
            }
        }
    }

    // Adjust TLS vaddr if relocated
    let tls_info = if let Some(mut tls) = tls_info {
        if is_pie || needs_relocation {
            tls.vaddr = mem_base_addr + (tls.vaddr - min_vaddr);
        }
        Some(tls)
    } else {
        None
    };

    // IMPORTANT: For relocated EXEC files, we need to fix up pointers in .data/.got sections
    // that point to code/data addresses. Since there are no RELATIVE relocations in the ELF,
    // we need to manually scan and fix known patterns.
    // For now, we fix the critical GOT entries used by __libc_start_main
    if !is_pie && needs_relocation {
        let reloc_offset = mem_base_addr - min_vaddr;

        // Scan all non-executable PT_LOAD segments.
        // Function pointers can live in read-only data, not only writable segments.
        for phdr in all_phdrs.iter() {
            if phdr.p_type != PT_LOAD || (phdr.p_flags & PF_X) != 0 {
                continue;
            }
            let seg_addr = if is_pie || needs_relocation {
                mem_base_addr + (phdr.p_vaddr - min_vaddr)
            } else {
                phdr.p_vaddr
            };
            if seg_addr < mem_base_addr || seg_addr + phdr.p_memsz > mem_base_addr + mem_base.len() as u64 {
                continue;
            }
            let seg_off = (seg_addr - mem_base_addr) as usize;
            let seg_size = phdr.p_memsz as usize;
            let seg_end = seg_off + seg_size;

            for ptr_offset in (seg_off..seg_end).step_by(8) {
                if ptr_offset + 8 > seg_end {
                    break;
                }
                let mut ptr_bytes = [0u8; 8];
                ptr_bytes.copy_from_slice(&mem_base[ptr_offset..ptr_offset + 8]);
                let ptr = u64::from_le_bytes(ptr_bytes);

                // Heuristic for pointers that still reference pre-relocation addresses.
                if ptr >= min_vaddr && ptr < min_vaddr + 0x100000 {
                    let new_ptr = ptr + reloc_offset;
                    mem_base[ptr_offset..ptr_offset + 8].copy_from_slice(&new_ptr.to_le_bytes());
                }
            }
        }
    }

    Ok((entry, tls_info))
}
