use std::{fs, ops::Range, path::Path};

use snafu::{Whatever, FromString};

const EI_CLASS: usize = 4;
const EI_DATA: usize = 5;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const PT_LOAD: u32 = 1;
const EHDR_SIZE: usize = 64;
const PHDR_SIZE: usize = 56;

pub fn load_elf_to_mem(
  path: impl AsRef<Path>,
  data: &mut [u8],
  mem_base: u64,
) -> Result<usize, Whatever> {
  let path = path.as_ref();
  let buf = fs::read(path).map_err(|e| {
    Whatever::without_source(format!("[SimDRAM_bb] Cannot open ELF: {}: {e}", path.display()))
  })?;

  if buf.len() < EHDR_SIZE {
    return fail(format!(
      "[SimDRAM_bb] ELF header is too short: {}",
      path.display()
    ));
  }
  if &buf[0..4] != b"\x7fELF" {
    return fail(format!("[SimDRAM_bb] Not a valid ELF file: {}", path.display()));
  }
  if buf[EI_CLASS] != ELFCLASS64 {
    return fail("[SimDRAM_bb] Only ELF64 supported");
  }
  if buf[EI_DATA] != ELFDATA2LSB {
    return fail("[SimDRAM_bb] Only little-endian ELF supported");
  }

  let phoff = usize::try_from(read_u64(&buf, 32, "e_phoff")?)
    .map_err(|_| Whatever::without_source("[SimDRAM_bb] e_phoff does not fit usize".to_string()))?;
  let phentsize = usize::from(read_u16(&buf, 54, "e_phentsize")?);
  let phnum = usize::from(read_u16(&buf, 56, "e_phnum")?);
  if phentsize != PHDR_SIZE {
    return fail(format!(
      "[SimDRAM_bb] unexpected ELF64 program header size: {phentsize}"
    ));
  }

  let mem_size = u64::try_from(data.len())
    .map_err(|_| Whatever::without_source("[SimDRAM_bb] memory size does not fit u64".to_string()))?;
  let mem_end = mem_base
    .checked_add(mem_size)
    .ok_or_else(|| Whatever::without_source("[SimDRAM_bb] memory range overflow".to_string()))?;
  let mut loaded = 0usize;

  for i in 0..phnum {
    let ph = phoff
      .checked_add(i.checked_mul(phentsize).ok_or_else(|| {
        Whatever::without_source("[SimDRAM_bb] program header index overflow".to_string())
      })?)
      .ok_or_else(|| Whatever::without_source("[SimDRAM_bb] program header offset overflow".to_string()))?;
    let ph = mem_range(buf.len(), ph, phentsize, "program header")?.start;
    let p_type = read_u32(&buf, ph, "p_type")?;
    if p_type != PT_LOAD {
      continue;
    }

    let p_offset = read_u64(&buf, ph + 8, "p_offset")?;
    let p_paddr = read_u64(&buf, ph + 24, "p_paddr")?;
    let p_filesz = read_u64(&buf, ph + 32, "p_filesz")?;
    let p_memsz = read_u64(&buf, ph + 40, "p_memsz")?;
    if p_filesz == 0 {
      continue;
    }
    if p_filesz > p_memsz {
      return fail(format!(
        "[SimDRAM_bb] Segment paddr=0x{p_paddr:x} filesz=0x{p_filesz:x} exceeds memsz=0x{p_memsz:x}"
      ));
    }

    let seg_end = p_paddr
      .checked_add(p_memsz)
      .ok_or_else(|| Whatever::without_source("[SimDRAM_bb] segment range overflow".to_string()))?;
    if p_paddr < mem_base || seg_end > mem_end {
      return fail(format!(
        "[SimDRAM_bb] Segment paddr=0x{p_paddr:x} size=0x{p_memsz:x} outside mem [0x{mem_base:x}, 0x{mem_end:x})"
      ));
    }

    let src = file_range(&buf, p_offset, p_filesz, "segment data")?;
    let dst_start = usize::try_from(p_paddr - mem_base)
      .map_err(|_| Whatever::without_source("[SimDRAM_bb] segment offset does not fit usize".to_string()))?;
    let file_sz = usize::try_from(p_filesz)
      .map_err(|_| Whatever::without_source("[SimDRAM_bb] segment filesz does not fit usize".to_string()))?;
    let mem_sz = usize::try_from(p_memsz)
      .map_err(|_| Whatever::without_source("[SimDRAM_bb] segment memsz does not fit usize".to_string()))?;
    let dst_file = mem_range(data.len(), dst_start, file_sz, "segment destination")?;
    data[dst_file].copy_from_slice(&buf[src]);

    if mem_sz > file_sz {
      let zero_start = dst_start
        .checked_add(file_sz)
        .ok_or_else(|| Whatever::without_source("[SimDRAM_bb] zero fill offset overflow".to_string()))?;
      let dst_zero = mem_range(data.len(), zero_start, mem_sz - file_sz, "segment zero fill")?;
      data[dst_zero].fill(0);
    }

    loaded = loaded
      .checked_add(file_sz)
      .ok_or_else(|| Whatever::without_source("[SimDRAM_bb] loaded byte count overflow".to_string()))?;
  }

  println!("[SimDRAM_bb] Loaded ELF '{}': {} bytes", path.display(), loaded);
  Ok(loaded)
}

fn fail<T>(msg: impl Into<String>) -> Result<T, Whatever> {
  Err(Whatever::without_source(msg.into()))
}

fn read_u16(buf: &[u8], at: usize, name: &str) -> Result<u16, Whatever> {
  Ok(u16::from_le_bytes(bytes(buf, at, name)?))
}

fn read_u32(buf: &[u8], at: usize, name: &str) -> Result<u32, Whatever> {
  Ok(u32::from_le_bytes(bytes(buf, at, name)?))
}

fn read_u64(buf: &[u8], at: usize, name: &str) -> Result<u64, Whatever> {
  Ok(u64::from_le_bytes(bytes(buf, at, name)?))
}

fn bytes<const N: usize>(buf: &[u8], at: usize, name: &str) -> Result<[u8; N], Whatever> {
  let range = mem_range(buf.len(), at, N, name)?;
  let mut out = [0; N];
  out.copy_from_slice(&buf[range]);
  Ok(out)
}

fn file_range(buf: &[u8], at: u64, len: u64, name: &str) -> Result<Range<usize>, Whatever> {
  let at = usize::try_from(at)
    .map_err(|_| Whatever::without_source(format!("[SimDRAM_bb] {name} offset does not fit usize")))?;
  let len = usize::try_from(len)
    .map_err(|_| Whatever::without_source(format!("[SimDRAM_bb] {name} size does not fit usize")))?;
  mem_range(buf.len(), at, len, name)
}

fn mem_range(size: usize, at: usize, len: usize, name: &str) -> Result<Range<usize>, Whatever> {
  let end = at
    .checked_add(len)
    .ok_or_else(|| Whatever::without_source(format!("[SimDRAM_bb] {name} range overflow")))?;
  if end > size {
    return fail(format!("[SimDRAM_bb] {name} range outside buffer"));
  }
  Ok(at..end)
}
