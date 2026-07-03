//===- decode.rs - Instruction dispatch ------------------------------------===//
//
// ISA decode — funct7 and rs1/rs2 fields match `bb-tests/workloads/lib/bbhw/isa/isa.h`
// (`FIELD`, `BB_BANK0`..`BB_BANK2`, `BB_ITER`).
//
//===-----------------------------------------------------------------===//-----===//

use super::super::bank::{BankMap, BANK_NUM};

// Re-export the active chip instruction set.
pub use super::active_chip::{cycles_after_issue, execute_known};

#[inline]
pub fn rs1_b0(xs1: u64) -> u64 {
    xs1 & 0x3ff
}

#[inline]
pub fn rs1_b1(xs1: u64) -> u64 {
    (xs1 >> 10) & 0x3ff
}

#[inline]
pub fn rs1_b2(xs1: u64) -> u64 {
    (xs1 >> 20) & 0x3ff
}

/// `BB_ITER` — bits [63:30].
#[inline]
pub fn rs1_iter(xs1: u64) -> u64 {
    xs1 >> 30
}

#[inline]
pub fn xs2_mem_stride(xs2: u64) -> (u64, u64) {
    let mem = xs2 & ((1u64 << 39) - 1);
    let stride = (xs2 >> 39) & 0x7_ffff;
    (mem, stride)
}

#[inline]
pub fn xs2_mset(xs2: u64) -> (u64, u64, u64) {
    let row = xs2 & 0x1f;
    let col = (xs2 >> 5) & 0x1f;
    let alloc = (xs2 >> 10) & 1;
    (row, col, alloc)
}

/// the bank field in the instruction is **vbank_id**; parse it to physical slot index before accessing `banks`.
#[inline]
pub fn pbank(bm: &BankMap, vbank: u64) -> usize {
    pbank_group(bm, vbank, 0)
}

#[inline]
pub fn pbank_group(bm: &BankMap, vbank: u64, group: u64) -> usize {
    if vbank >= BANK_NUM as u64 {
        panic!("pbank: invalid vbank_id {vbank}");
    }
    bm.resolve_group(vbank as u32, group as u32)
        .unwrap_or_else(|| panic!("pbank: vbank {vbank} group {group} not mapped"))
}
