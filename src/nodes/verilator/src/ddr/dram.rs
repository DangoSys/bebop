// DRAM simulation and ELF loading

use goblin::elf::{program_header::PT_LOAD, Elf};
use memmap2::MmapMut;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::sync::Mutex;
use std::sync::OnceLock;

static MEMORY: OnceLock<Mutex<HashMap<u64, MmapMut>>> = OnceLock::new();

fn get_memory() -> &'static Mutex<HashMap<u64, MmapMut>> {
    MEMORY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn init_memory(mem_base: u64, mem_size: usize) -> io::Result<()> {
    let mmap = MmapMut::map_anon(mem_size)?;
    get_memory().lock().unwrap().insert(mem_base, mmap);
    Ok(())
}

pub fn load_elf(elf_path: &Path, mem_base: u64, mem_size: usize) -> io::Result<()> {
    let mut file = File::open(elf_path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    let elf = Elf::parse(&buffer)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("ELF parse error: {}", e)))?;

    if elf.header.e_machine != goblin::elf::header::EM_RISCV {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Not a RISC-V ELF file"));
    }

    let mut memory = get_memory().lock().unwrap();
    let mmap = memory
        .get_mut(&mem_base)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Memory not initialized"))?;

    let mut loaded = 0;
    for ph in &elf.program_headers {
        if ph.p_type != PT_LOAD || ph.p_filesz == 0 {
            continue;
        }

        let vaddr = ph.p_paddr;
        if vaddr < mem_base || vaddr + ph.p_memsz > mem_base + mem_size as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Segment paddr=0x{:x} size=0x{:x} outside mem [0x{:x}, 0x{:x})",
                    vaddr,
                    ph.p_memsz,
                    mem_base,
                    mem_base + mem_size as u64
                ),
            ));
        }

        let offset = (vaddr - mem_base) as usize;
        let file_offset = ph.p_offset as usize;
        let file_size = ph.p_filesz as usize;
        let mem_size_seg = ph.p_memsz as usize;

        // Copy file data
        mmap[offset..offset + file_size].copy_from_slice(&buffer[file_offset..file_offset + file_size]);

        // Zero BSS
        if mem_size_seg > file_size {
            mmap[offset + file_size..offset + mem_size_seg].fill(0);
        }

        loaded += file_size;
    }

    println!("[DRAM] Loaded ELF '{}': {} bytes", elf_path.display(), loaded);
    Ok(())
}

// Memory read/write for DPI-C (if needed)
#[allow(dead_code)]
pub fn mem_read(addr: u64, size: usize) -> Option<Vec<u8>> {
    let memory = get_memory().lock().unwrap();
    for (&base, mmap) in memory.iter() {
        if addr >= base && addr + size as u64 <= base + mmap.len() as u64 {
            let offset = (addr - base) as usize;
            return Some(mmap[offset..offset + size].to_vec());
        }
    }
    None
}

#[allow(dead_code)]
pub fn mem_write(addr: u64, data: &[u8]) -> bool {
    let mut memory = get_memory().lock().unwrap();
    for (&base, mmap) in memory.iter_mut() {
        if addr >= base && addr + data.len() as u64 <= base + mmap.len() as u64 {
            let offset = (addr - base) as usize;
            mmap[offset..offset + data.len()].copy_from_slice(data);
            return true;
        }
    }
    false
}
