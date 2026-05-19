//===- 52_dequant.rs - DEQUANT instruction (dequantization) ----------------===//

use super::super::bank::{BANK_NUM, BANK_SIZE};
use super::decode::{pbank, rs1_b0, rs1_b2, rs1_iter};
use super::instruction::{ExecContext, Instruction};

pub struct Dequant;

impl Instruction for Dequant {
    const FUNCT: u32 = 52;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let src = rs1_b0(xs1);
        let dst = rs1_b2(xs1);
        let depth = rs1_iter(xs1) as usize;

        if src >= BANK_NUM as u64 || dst >= BANK_NUM as u64 {
            panic!("dequant: invalid bank_id");
        }

        let sc = ctx.cfgs[src as usize];
        let dc = ctx.cfgs[dst as usize];
        if !sc.allocated || !dc.allocated {
            panic!("dequant: bank not allocated");
        }

        let ps = pbank(ctx.bank_map, src);
        let pd = pbank(ctx.bank_map, dst);
        let scale = f32::from_bits((xs2 & 0xffff_ffff) as u32);

        // Support two modes:
        // 1. INT32 -> FP32: src_cols=1, dst_cols=1 (4 bytes -> 4 bytes)
        // 2. INT8 -> FP32: src_cols=1, dst_cols=4 (1 byte -> 4 bytes)
        match (sc.cols, dc.cols) {
            (1, 1) => {
                // INT32 -> FP32 mode
                for i in 0..depth {
                    let src_base = i * 64;
                    let dst_base = i * 64;
                    if src_base + 64 > BANK_SIZE || dst_base + 64 > BANK_SIZE {
                        panic!("dequant: out of range");
                    }
                    for j in 0..16 {
                        let off = src_base + j * 4;
                        let v = i32::from_le_bytes(ctx.banks[ps][off..off + 4].try_into().unwrap());
                        let o = ((v as f32) * scale).to_bits();
                        let dst_off = dst_base + j * 4;
                        ctx.banks[pd][dst_off..dst_off + 4].copy_from_slice(&o.to_le_bytes());
                    }
                }
            }
            (1, 4) => {
                // INT8 -> FP32 mode (original implementation)
                for i in 0..depth {
                    let src_base = i * 16;
                    let dst_base = i * 64;
                    if src_base + 16 > BANK_SIZE || dst_base + 64 > BANK_SIZE {
                        panic!("dequant: out of range");
                    }
                    for j in 0..16 {
                        let v = ctx.banks[ps][src_base + j] as i8;
                        let o = ((v as f32) * scale).to_bits();
                        let off = dst_base + j * 4;
                        ctx.banks[pd][off..off + 4].copy_from_slice(&o.to_le_bytes());
                    }
                }
            }
            _ => {
                panic!(
                    "dequant: unsupported layout src_cols={} dst_cols={}. Supported: (1,1) for INT32->FP32, (1,4) for INT8->FP32",
                    sc.cols, dc.cols
                );
            }
        }
        0
    }

    fn latency(xs1: u64, _xs2: u64) -> u64 {
        rs1_iter(xs1).max(1)
    }
}
