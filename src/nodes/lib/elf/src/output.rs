use crate::{loader::read_elf_header, symbols::read_shdrs};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

pub const OUTPUT_START: u32 = 0x42424f55;

/// Raw words read by a guest through its coherent memory path, followed by
/// the ordinary exit write. No guest comparison or numeric formatting.
#[derive(Debug)]
pub struct OutputCapture {
    bytes: usize,
    hart: Option<usize>,
    data: Vec<u8>,
}

impl OutputCapture {
    pub fn from_elf(path: &Path) -> Result<Option<Self>, String> {
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let header = read_elf_header(&mut file)?;
        if header.e_shnum == 0 {
            return Ok(None);
        }
        let sections = read_shdrs(&mut file, &header)?;
        let names = &sections[header.e_shstrndx as usize];
        file.seek(SeekFrom::Start(names.sh_offset)).map_err(|e| e.to_string())?;
        let mut strings = vec![0; names.sh_size as usize];
        file.read_exact(&mut strings).map_err(|e| e.to_string())?;
        for section in &sections {
            let name = strings[section.sh_name as usize..].split(|b| *b == 0).next().unwrap();
            if name != b".bbsim.capture_output" && name != b".bbsim.check_output" {
                continue;
            }
            if section.sh_size != 24 {
                return Err("output checks require one 24-byte record".into());
            }
            file.seek(SeekFrom::Start(section.sh_offset + 16))
                .map_err(|e| e.to_string())?;
            let mut size = [0; 8];
            file.read_exact(&mut size).map_err(|e| e.to_string())?;
            let bytes = usize::try_from(u64::from_le_bytes(size)).map_err(|e| e.to_string())?;
            return Ok(Some(Self {
                bytes,
                hart: None,
                data: Vec::with_capacity(bytes),
            }));
        }
        Ok(None)
    }

    pub fn push(&mut self, hart: usize, word: u32) -> Result<bool, String> {
        if self.hart.is_none() {
            if word != OUTPUT_START {
                return Ok(false);
            }
            self.hart = Some(hart);
            return Ok(true);
        }
        if self.hart != Some(hart) {
            return Err("output words came from different harts".into());
        }
        if self.data.len() == self.bytes {
            return Ok(false);
        }
        let count = (self.bytes - self.data.len()).min(4);
        self.data.extend_from_slice(&word.to_le_bytes()[..count]);
        Ok(true)
    }

    pub fn data(&self) -> Result<&[u8], String> {
        if self.hart.is_none() || self.data.len() != self.bytes {
            return Err("guest output is incomplete".into());
        }
        Ok(&self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_words_are_data_until_the_declared_size() {
        let mut output = OutputCapture {
            bytes: 6,
            hart: None,
            data: Vec::new(),
        };
        assert!(!output.push(0, 1).unwrap());
        assert!(output.data().is_err());
        assert!(output.push(2, OUTPUT_START).unwrap());
        assert!(output.push(2, OUTPUT_START).unwrap());
        assert!(output.data().is_err());
        assert!(output.push(2, 0xdead1234).unwrap());
        assert_eq!(output.data().unwrap(), &[0x55, 0x4f, 0x42, 0x42, 0x34, 0x12]);
        assert!(!output.push(2, 0).unwrap());
    }

    #[test]
    fn different_harts_cannot_mix_an_output() {
        let mut output = OutputCapture {
            bytes: 4,
            hart: None,
            data: Vec::new(),
        };
        assert!(output.push(1, OUTPUT_START).unwrap());
        assert!(output.push(0, 7).is_err());
    }
}
