//===- 51_quant.rs - QUANT instruction (quantization) ----------------------===//

use super::super::bank::{BANK_NUM, BANK_SIZE};
use super::decode::{pbank, rs1_b0, rs1_b2, rs1_iter};
use super::instruction::{ExecContext, Instruction};

pub struct Quant;

impl Instruction for Quant {
    const FUNCT: u32 = 51;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let src = rs1_b0(xs1);
        let dst = rs1_b2(xs1);
        let depth = rs1_iter(xs1) as usize;

        if src >= BANK_NUM as u64 || dst >= BANK_NUM as u64 {
            panic!("quant: invalid bank_id");
        }

        let sc = ctx.cfgs[src as usize];
        let dc = ctx.cfgs[dst as usize];
        if !sc.allocated || !dc.allocated {
            panic!("quant: bank not allocated");
        }
        if sc.cols != 4 || dc.cols != 1 {
            panic!("quant: unsupported layout src_cols={} dst_cols={}", sc.cols, dc.cols);
        }

        let ps = pbank(ctx.bank_map, src);
        let pd = pbank(ctx.bank_map, dst);
        let scale = f32::from_bits((xs2 & 0xffff_ffff) as u32);

        for i in 0..depth {
            let src_base = i * 64;
            let dst_base = i * 16;
            if src_base + 64 > BANK_SIZE || dst_base + 16 > BANK_SIZE {
                panic!("quant: out of range");
            }
            for j in 0..16 {
                let off = src_base + j * 4;
                let v = i32::from_le_bytes(ctx.banks[ps][off..off + 4].try_into().unwrap());
                let q = ((v as f32) * scale).round().clamp(-128.0, 127.0) as i8;
                ctx.banks[pd][dst_base + j] = q as u8;
            }
        }
        0
    }

    fn latency(xs1: u64, _xs2: u64) -> u64 {
        rs1_iter(xs1).max(1)
    }
}
