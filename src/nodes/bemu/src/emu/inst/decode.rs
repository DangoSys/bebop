//===- decode.rs - Instruction dispatch ------------------------------------===//
//
// ISA decode - funct7 and rs1/rs2 fields match `bb-tests/workloads/lib/bbhw/isa/isa.h`
// (`FIELD`, `BB_BANK0`..`BB_BANK2`, `BB_ITER`).
//
//===-----------------------------------------------------------------===//-----===//

use super::instruction::ExecContext;

pub use super::{cycles_after_issue, execute_known};

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

/// `BB_ITER` - bits [63:30].
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
pub fn xs2_mset(xs2: u64) -> (u64, u64, u64, bool) {
    let row = xs2 & 0x1f;
    let col = (xs2 >> 5) & 0x1f;
    let alloc = (xs2 >> 10) & 1;
    let clear = ((xs2 >> 11) & 1) != 0;
    (row, col, alloc, clear)
}

/// the bank field in the instruction is **vbank_id**; parse it to physical slot index before accessing `banks`.
#[inline]
pub fn pbank(ctx: &ExecContext, vbank: u64) -> usize {
    pbank_group(ctx, vbank, 0)
}

#[inline]
pub fn pbank_group(ctx: &ExecContext, vbank: u64, group: u64) -> usize {
    ctx.physical_bank(vbank, group)
}

pub const FUNCT7_MSET: u32 = 32;
pub const FUNCT7_MVIN_MMIO: u32 = 35;

#[derive(Clone, Copy)]
pub struct DmaRows { pub address: u64, pub depth: u64, pub groups: u64, pub stride: u64, pub selected_group: Option<u64> }
impl DmaRows {
    pub fn decode(xs1: u64, xs2: u64, groups: u64) -> Self {
        let (address, stride) = xs2_mem_stride(xs2);
        let depth = rs1_iter(xs1);
        assert!(depth > 0 && stride > 0 && groups > 0);
        let group = (xs2 >> 58) & 0x1f;
        let selected_group = if xs2 >> 63 != 0 {
            assert!(group < groups, "DMA selected group exceeds bank");
            Some(group)
        } else {
            assert!(group == 0, "DMA whole-bank mode has reserved group bits");
            None
        };
        Self { address, depth, groups: if selected_group.is_some() { 1 } else { groups }, stride, selected_group }
    }
    pub fn source(&self, row: u64, group: u64) -> u64 {
        self.address + row * self.groups * 16 * self.stride + group * 16
    }
}

#[derive(Clone, Copy)]
pub struct Mvin2dGeometry {
    pub bank: u64, pub height: u64, pub address: u64, pub pixel_bytes: u64,
    pub source_width: u64, pub dst_base: u64, pub width: u64, pub valid_bytes: u64,
}
impl Mvin2dGeometry {
    pub fn decode(xs1: u64, xs2: u64) -> Self {
        let value = Self { bank: rs1_b2(xs1), height: rs1_iter(xs1),
            address: (xs2 & 0x000f_ffff_ffff) << 3, pixel_bytes: ((xs2 >> 36) & 0x7f) * 8,
            source_width: (xs2 >> 43) & 0x3ff, dst_base: (xs2 >> 53) & 0x3f,
            width: ((xs2 >> 59) & 7) + 1, valid_bytes: if (xs2 >> 62) & 1 == 0 { 16 } else { 8 } };
        assert!(value.height > 0 && value.pixel_bytes > 0 && value.source_width > 0
            && value.valid_bytes <= value.pixel_bytes, "mvin_2d: invalid geometry");
        assert!(xs2 >> 63 == 0 && xs1 & 0x000f_ffff == 0, "mvin_2d: reserved bits");
        value
    }
    pub fn source(&self, row: u64, column: u64) -> u64 {
        self.address + row * self.source_width * self.pixel_bytes + column * self.pixel_bytes
    }
}
