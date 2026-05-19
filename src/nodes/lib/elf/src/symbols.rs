use crate::constants::*;
use crate::types::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub fn read_shdrs(file: &mut File, ehdr: &Elf64Ehdr) -> Result<Vec<Elf64Shdr>, String> {
    file.seek(SeekFrom::Start(ehdr.e_shoff))
        .map_err(|e| format!("Failed to seek to section headers: {}", e))?;
    let mut shdrs = Vec::with_capacity(ehdr.e_shnum as usize);
    for _ in 0..ehdr.e_shnum {
        let mut shdr_bytes = [0u8; std::mem::size_of::<Elf64Shdr>()];
        file.read_exact(&mut shdr_bytes)
            .map_err(|e| format!("Failed to read section header: {}", e))?;
        // SAFETY: shdr_bytes is exactly sizeof(Elf64Shdr) bytes from disk; Elf64Shdr is
        // #[repr(C)] with only POD fields, so any byte pattern is valid.
        let shdr: Elf64Shdr = unsafe { std::ptr::read(shdr_bytes.as_ptr() as *const _) };
        shdrs.push(shdr);
    }
    Ok(shdrs)
}

pub fn read_ifunc_map(file: &mut File, shdrs: &[Elf64Shdr]) -> Result<HashMap<u64, u64>, String> {
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
            // SAFETY: sym_bytes is exactly sizeof(Elf64Sym) bytes from disk; Elf64Sym is
            // #[repr(C)] with only POD fields, so any byte pattern is valid.
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
