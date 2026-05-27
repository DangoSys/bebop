//===- mmio.rs - MMIO bank read operations ---------------------------------===//
//
// Provides MMIO read functionality for Ball instructions.
// Balls can read per-element or per-block metadata (e.g., scales) from MMIO.
//
//===-----------------------------------------------------------------===//

use super::super::inst::instruction::MmioRegion;

/// Read a byte from MMIO banks.
///
/// # Arguments
/// * `mmio_banks` - 16 MMIO banks, each 1024 bytes (16 banks × 1KB = 16KB total)
/// * `mmio_region_table` - Region table mapping main banks to MMIO regions
/// * `meta_bank` - Main bank ID whose MMIO region to use
/// * `rel_addr` - Relative byte address within the MMIO region
///
/// # Returns
/// The byte value at the specified MMIO address, or 0 if invalid.
pub fn mmio_read_byte(
    mmio_banks: &[[u8; 1024]; 16],
    mmio_region_table: &[MmioRegion; 32],
    meta_bank: usize,
    rel_addr: usize,
) -> u8 {
    if meta_bank >= 32 {
        eprintln!("[WARN] mmio_read_byte: invalid meta_bank {}", meta_bank);
        return 0;
    }

    let region = &mmio_region_table[meta_bank];
    if !region.valid {
        eprintln!("[WARN] mmio_read_byte: no MMIO region bound to bank {}", meta_bank);
        return 0;
    }

    // Compute absolute MMIO byte address
    let abs_addr = region.mmio_addr as usize + rel_addr;

    // Check bounds (16 banks × 1024 bytes = 16384 bytes total)
    if abs_addr >= 16384 {
        eprintln!("[WARN] mmio_read_byte: address 0x{:x} out of range", abs_addr);
        return 0;
    }

    let bank_idx = abs_addr / 1024;
    let bank_offset = abs_addr % 1024;

    mmio_banks[bank_idx][bank_offset]
}

/// Read a 16-bit word from MMIO banks (little-endian).
pub fn mmio_read_u16(
    mmio_banks: &[[u8; 1024]; 16],
    mmio_region_table: &[MmioRegion; 32],
    meta_bank: usize,
    rel_addr: usize,
) -> u16 {
    let lo = mmio_read_byte(mmio_banks, mmio_region_table, meta_bank, rel_addr) as u16;
    let hi = mmio_read_byte(mmio_banks, mmio_region_table, meta_bank, rel_addr + 1) as u16;
    lo | (hi << 8)
}

/// Read a 32-bit word from MMIO banks (little-endian).
pub fn mmio_read_u32(
    mmio_banks: &[[u8; 1024]; 16],
    mmio_region_table: &[MmioRegion; 32],
    meta_bank: usize,
    rel_addr: usize,
) -> u32 {
    let lo = mmio_read_u16(mmio_banks, mmio_region_table, meta_bank, rel_addr) as u32;
    let hi = mmio_read_u16(mmio_banks, mmio_region_table, meta_bank, rel_addr + 2) as u32;
    lo | (hi << 16)
}
