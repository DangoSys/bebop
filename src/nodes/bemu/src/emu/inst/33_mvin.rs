//===- 33_mvin.rs - MVIN instruction (memory to bank) ----------------------===//

use super::super::bank::{mem_read, BANK_NUM, BANK_SIZE, MATRIX_SIZE};
use super::decode::{pbank, pbank_group, rs1_b0, rs1_iter, xs2_mem_stride};
use super::instruction::{ExecContext, Instruction};

pub struct Mvin;

impl Instruction for Mvin {
    const FUNCT: u32 = 33;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let bank_id = rs1_b0(xs1);
        let depth = rs1_iter(xs1);
        let (mem_addr, stride) = xs2_mem_stride(xs2);

        if std::env::var("BEMU_RTRACE").is_ok() {
            eprintln!("[RTRACE] mvin: DRAM[0x{:x}] stride={} -> bank{} depth={}",
                mem_addr, stride, bank_id, depth);
        }

        if bank_id >= BANK_NUM as u64 {
            panic!("mvin: invalid bank_id {bank_id}");
        }

        let bi = bank_id as usize;
        if !ctx.cfgs[bi].allocated {
            panic!("mvin: bank {bank_id} not allocated");
        }

        let cols = ctx.cfgs[bi].cols;
        let groups = cols.max(1) as usize;
        let actual_stride = if stride == 0 { 1 } else { stride };

        if groups > 1 {
            for row in 0..depth as usize {
                for group in 0..groups {
                    let p = pbank_group(ctx.bank_map, bank_id, group as u64);
                    let bank_offset = row * 16;
                    if bank_offset + 16 > BANK_SIZE {
                        panic!("mvin: bank range: bank_offset={bank_offset} line_bytes=16 depth={depth}");
                    }
                    let addr = mem_addr
                        + row as u64 * groups as u64 * 16 * actual_stride
                        + group as u64 * 16;
                    for j in 0..16 {
                        ctx.banks[p][bank_offset + j] = mem_read(ctx.memory, addr + j as u64);
                    }
                }
            }
        } else {
            let p = pbank(ctx.bank_map, bank_id);
            let matrix_mode_acc = cols == 4 && depth <= MATRIX_SIZE as u64;
            let line_bytes = if matrix_mode_acc { 64usize } else { 16usize };

            for i in 0..depth {
                let addr = mem_addr + i * line_bytes as u64 * actual_stride;
                let bank_offset = (i as usize) * line_bytes;
                if bank_offset + line_bytes > BANK_SIZE {
                    panic!("mvin: bank range: bank_offset={bank_offset} line_bytes={line_bytes} depth={depth}");
                }
                for j in 0..line_bytes {
                    ctx.banks[p][bank_offset + j] = mem_read(ctx.memory, addr + j as u64);
                }
            }
        }
        0
    }

    fn latency(xs1: u64, _xs2: u64) -> u64 {
        rs1_iter(xs1).max(1)
    }
}
