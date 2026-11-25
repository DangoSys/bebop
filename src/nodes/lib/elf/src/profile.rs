use crate::{loader::read_elf_header, symbols::read_shdrs};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

#[derive(Clone, Copy)]
pub struct CpuProfileScope {
    pub id: u64,
    pub begin: u64,
    /// Zero means the architectural return address at function entry.
    pub end: u64,
}

pub fn read_cpu_profile_scopes(path: &Path) -> Result<Vec<CpuProfileScope>, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let header = read_elf_header(&mut file)?;
    if header.e_shnum == 0 {
        return Ok(Vec::new());
    }
    let sections = read_shdrs(&mut file, &header)?;
    let names = &sections[header.e_shstrndx as usize];
    file.seek(SeekFrom::Start(names.sh_offset)).map_err(|e| e.to_string())?;
    let mut strings = vec![0; names.sh_size as usize];
    file.read_exact(&mut strings).map_err(|e| e.to_string())?;
    let mut scopes = Vec::new();
    for section in &sections {
        if strings[section.sh_name as usize..].split(|b| *b == 0).next().unwrap() != b".bbsim.profile" {
            continue;
        }
        if section.sh_size % 24 != 0 {
            return Err("CPU profile scopes require 24-byte records".into());
        }
        file.seek(SeekFrom::Start(section.sh_offset))
            .map_err(|e| e.to_string())?;
        let mut bytes = vec![0; section.sh_size as usize];
        file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
        for record in bytes.chunks_exact(24) {
            scopes.push(CpuProfileScope {
                id: u64::from_le_bytes(record[0..8].try_into().unwrap()),
                begin: u64::from_le_bytes(record[8..16].try_into().unwrap()),
                end: u64::from_le_bytes(record[16..24].try_into().unwrap()),
            });
        }
    }
    Ok(scopes)
}
