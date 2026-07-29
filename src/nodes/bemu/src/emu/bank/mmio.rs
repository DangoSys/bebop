//===- mmio.rs - MMIO bank read operations ---------------------------------===//
//
// Provides MMIO read functionality for Ball instructions.
// Balls can read per-element or per-block metadata (e.g., scales) from MMIO.
//
//===-----------------------------------------------------------------===//

use super::super::inst::instruction::MmioRegion;
use super::{mmio_bank_size, mmio_enable, mmio_total_size};

/// Read a byte from MMIO banks.
///
/// # Arguments
/// * `mmio_banks` - MMIO banks sized from the active chip memdomain TOML
/// * `mmio_region_table` - Region table mapping main banks to MMIO regions
/// * `meta_bank` - Main bank ID whose MMIO region to use
/// * `rel_addr` - Relative byte address within the MMIO region
///
/// # Returns
/// The byte value at the specified MMIO address, or 0 if invalid.
#[allow(dead_code)]
pub fn mmio_read_byte(
    mmio_banks: &[Vec<u8>],
    mmio_region_table: &[MmioRegion],
    meta_bank: usize,
    rel_addr: usize,
) -> u8 {
    if !mmio_enable() {
        panic!("mmio_read_byte: MMIO is disabled for this BEMU chip config");
    }

    if meta_bank >= mmio_region_table.len() {
        eprintln!("[WARN] mmio_read_byte: invalid meta_bank {}", meta_bank);
        return 0;
    }

    let region = &mmio_region_table[meta_bank];
    if !region.valid {
        eprintln!("[WARN] mmio_read_byte: no MMIO region bound to bank {}", meta_bank);
        return 0;
    }

    let size_bytes = region.size_rows as usize * mmio_bank_size();
    if rel_addr >= size_bytes {
        eprintln!(
            "[WARN] mmio_read_byte: relative address 0x{:x} out of region size 0x{:x}",
            rel_addr, size_bytes
        );
        return 0;
    }

    let abs_addr = region.mmio_addr as usize + rel_addr;

    if abs_addr >= mmio_total_size() {
        eprintln!("[WARN] mmio_read_byte: address 0x{:x} out of range", abs_addr);
        return 0;
    }

    let bank_idx = abs_addr / mmio_bank_size();
    let bank_offset = abs_addr % mmio_bank_size();

    mmio_banks[bank_idx][bank_offset]
}
