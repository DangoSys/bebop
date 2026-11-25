use crate::{loader::read_elf_header, symbols::read_shdrs};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

/// Static ELF records: actual address, expected address, byte count.
/// The producer must complete its writes to the memory exposed by `read`.
pub fn check_memory(
    path: &Path,
    reference_elf: Option<&Path>,
    output: Option<&[u8]>,
    mut read: impl FnMut(u64, &mut [u8]) -> Result<(), String>,
) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let header = read_elf_header(&mut file)?;
    if header.e_shnum == 0 {
        return Ok(());
    }
    let sections = read_shdrs(&mut file, &header)?;
    let names = &sections[header.e_shstrndx as usize];
    file.seek(SeekFrom::Start(names.sh_offset)).map_err(|e| e.to_string())?;
    let mut strings = vec![0; names.sh_size as usize];
    file.read_exact(&mut strings).map_err(|e| e.to_string())?;
    let mut reference_image = Vec::new();
    for section in &sections {
        let name = strings[section.sh_name as usize..].split(|b| *b == 0).next().unwrap();
        let streamed = name == b".bbsim.capture_output" || name == b".bbsim.check_output";
        let capture = name == b".bbsim.capture" || name == b".bbsim.capture_output";
        if name != b".bbsim.check" && !capture && !streamed {
            continue;
        }
        if capture {
            reference_elf.ok_or("capture records require an output ELF path")?;
            if reference_image.is_empty() {
                reference_image = std::fs::read(path).map_err(|e| e.to_string())?;
            }
            let at = (names.sh_offset + u64::from(section.sh_name)) as usize;
            reference_image[at..at + name.len()].fill(0);
            let checked: &[u8] = if streamed {
                b".bbsim.check_output"
            } else {
                b".bbsim.check"
            };
            reference_image[at..at + checked.len()].copy_from_slice(checked);
        }
        if header.e_type != 2 || section.sh_size % 24 != 0 {
            return Err("memory checks require an executable ELF and 24-byte records".into());
        }
        file.seek(SeekFrom::Start(section.sh_offset))
            .map_err(|e| e.to_string())?;
        let mut records = vec![0; section.sh_size as usize];
        file.read_exact(&mut records).map_err(|e| e.to_string())?;
        for record in records.chunks_exact(24) {
            let actual = u64::from_le_bytes(record[0..8].try_into().unwrap());
            let expected = u64::from_le_bytes(record[8..16].try_into().unwrap());
            let size =
                usize::try_from(u64::from_le_bytes(record[16..24].try_into().unwrap())).map_err(|e| e.to_string())?;
            let mut observed = vec![0; size];
            if streamed {
                let data = output.ok_or("guest output was not captured")?;
                if data.len() != size {
                    return Err("guest output size does not match its ELF record".into());
                }
                observed.copy_from_slice(data);
            } else {
                read(actual, &mut observed)?;
            }
            if capture {
                let target = sections
                    .iter()
                    .find(|s| {
                        s.sh_type != crate::SHT_NOBITS
                            && expected >= s.sh_addr
                            && expected + size as u64 <= s.sh_addr + s.sh_size
                    })
                    .ok_or("captured reference must occupy file-backed ELF data")?;
                let at = (target.sh_offset + expected - target.sh_addr) as usize;
                reference_image[at..at + size].copy_from_slice(&observed);
                println!("Memory reference captured: {size} bytes at 0x{actual:x}");
                continue;
            }
            let mut reference = vec![0; size];
            if streamed {
                let target = sections
                    .iter()
                    .find(|s| {
                        s.sh_type != crate::SHT_NOBITS
                            && expected >= s.sh_addr
                            && expected + size as u64 <= s.sh_addr + s.sh_size
                    })
                    .ok_or("output reference must occupy file-backed ELF data")?;
                file.seek(SeekFrom::Start(target.sh_offset + expected - target.sh_addr))
                    .map_err(|e| e.to_string())?;
                file.read_exact(&mut reference).map_err(|e| e.to_string())?;
            } else {
                read(expected, &mut reference)?;
            }
            if let Some(first) = observed.iter().zip(&reference).position(|(a, b)| a != b) {
                if streamed {
                    return Err(format!(
                        "Output check failed at byte {first}: actual={}, expected={}",
                        observed[first], reference[first]
                    ));
                }
                return Err(format!(
                    "Memory check failed at 0x{actual:x}+{first}: actual={}, expected={}",
                    observed[first], reference[first]
                ));
            }
            println!("Memory check passed: {size} bytes at 0x{actual:x}");
        }
    }
    if !reference_image.is_empty() {
        std::fs::write(reference_elf.unwrap(), reference_image).map_err(|e| e.to_string())?;
    }
    Ok(())
}
