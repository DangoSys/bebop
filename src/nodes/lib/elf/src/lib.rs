use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

// ELF constants
const EI_MAG0: usize = 0;
const EI_MAG1: usize = 1;
const EI_MAG2: usize = 2;
const EI_MAG3: usize = 3;
const ELFMAG0: u8 = 0x7f;
const ELFMAG1: u8 = b'E';
const ELFMAG2: u8 = b'L';
const ELFMAG3: u8 = b'F';

const PT_LOAD: u32 = 1;

#[repr(C)]
struct Elf64Ehdr {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

/// Load ELF file into memory
/// Returns entry point address
pub fn load_elf(
    path: &str,
    mem_base: &mut [u8],
    mem_base_addr: u64,
) -> Result<u64, String> {
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

    // Load program headers
    file.seek(SeekFrom::Start(ehdr.e_phoff))
        .map_err(|e| format!("Failed to seek to program headers: {}", e))?;

    for _ in 0..ehdr.e_phnum {
        let mut phdr_bytes = [0u8; std::mem::size_of::<Elf64Phdr>()];
        file.read_exact(&mut phdr_bytes)
            .map_err(|e| format!("Failed to read program header: {}", e))?;
        let phdr: Elf64Phdr = unsafe { std::ptr::read(phdr_bytes.as_ptr() as *const _) };

        if phdr.p_type == PT_LOAD {
            let addr = phdr.p_paddr;
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

    Ok(ehdr.e_entry)
}
