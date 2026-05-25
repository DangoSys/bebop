//===- 34_mmio_set.rs - MMIO_SET instruction (MMIO region setup) -----------===//
//
// Configures an MMIO region for a main bank.
//
// rs1[9:0]:    main_bank (BANK0)
// rs2[15:0]:   mmio_addr (16-bit MMIO byte address)
// rs2[23:16]:  size_rows (8-bit size in rows; 0 invalidates the region)
//
//===-----------------------------------------------------------------===//-----===//

use super::decode::rs1_b0;
use super::instruction::{ExecContext, Instruction, MmioRegion};

pub struct MmioSet;

impl Instruction for MmioSet {
    const FUNCT: u32 = 34;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let main_bank = rs1_b0(xs1) as usize;
        let mmio_addr = (xs2 & 0xFFFF) as u16;
        let size_rows = ((xs2 >> 16) & 0xFF) as u8;

        if std::env::var("BEMU_RTRACE").is_ok() {
            eprintln!(
                "[RTRACE] mmio_set: main_bank={} mmio_addr=0x{:x} size_rows={}",
                main_bank, mmio_addr, size_rows
            );
        }

        if main_bank >= 32 {
            panic!("mmio_set: invalid main_bank {}", main_bank);
        }

        if size_rows == 0 {
            ctx.mmio_region_table[main_bank].valid = false;
        } else {
            ctx.mmio_region_table[main_bank] = MmioRegion {
                valid: true,
                mmio_addr,
                size_rows,
            };
        }
        0
    }

    fn latency(_xs1: u64, _xs2: u64) -> u64 {
        1
    }
}
