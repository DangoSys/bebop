//===- 48_im2col.rs - IM2COL instruction -----------------------------------===//

use super::super::bank::{BankConfig, BankMap, BANK_NUM};
use super::decode::{pbank, rs1_b0, rs1_b2};
use super::instruction::{ExecContext, Instruction};

pub struct Im2col;

impl Instruction for Im2col {
    const FUNCT: u32 = 48;

    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
        let op1 = rs1_b0(xs1);
        let wr = rs1_b2(xs1);

        if op1 >= BANK_NUM as u64 || wr >= BANK_NUM as u64 {
            panic!("im2col: invalid bank_id");
        }
        if !ctx.cfgs[op1 as usize].allocated || !ctx.cfgs[wr as usize].allocated {
            panic!("im2col: bank not allocated");
        }
        if op1 == wr {
            panic!("im2col: op1 and wr must differ");
        }

        let kcol = (xs2 & 0xF) as usize;
        let krow = ((xs2 >> 4) & 0xF) as usize;
        let incol = ((xs2 >> 8) & 0x1F) as usize;
        let inrow = ((xs2 >> 13) & 0x3FF) as usize;
        let startcol = ((xs2 >> 23) & 0x1F) as usize;
        let startrow = ((xs2 >> 28) & 0x3FF) as usize;

        if kcol == 0 || krow == 0 || incol == 0 || inrow == 0 {
            panic!("im2col: invalid shape (zero dim)");
        }
        if incol < kcol || inrow < krow {
            panic!("im2col: kernel larger than input");
        }

        let row_end = inrow - krow;
        let col_end = incol - kcol;
        if startrow > row_end || startcol > col_end {
            panic!("im2col: invalid start window");
        }

        let po = pbank(ctx.bank_map, op1);
        let pw = pbank(ctx.bank_map, wr);
        let (srcb, dstb): (&[u8], &mut [u8]) = if po < pw {
            let (l, r) = ctx.banks.split_at_mut(pw);
            (&l[po], &mut r[0])
        } else {
            let (l, r) = ctx.banks.split_at_mut(po);
            (&r[0], &mut l[pw])
        };

        let mut out = 0usize;
        for r in startrow..=row_end {
            for c in startcol..=col_end {
                for kr in 0..krow {
                    for kc in 0..kcol {
                        let src = r * incol + c + kr * incol + kc;
                        if src >= srcb.len() || out >= dstb.len() {
                            panic!("im2col: range src={src} out={out}");
                        }
                        dstb[out] = srcb[src];
                        out += 1;
                    }
                }
            }
        }
        0
    }

    fn latency(_xs1: u64, xs2: u64) -> u64 {
        let kcol = (xs2 & 0xF) as u64;
        let krow = ((xs2 >> 4) & 0xF) as u64;
        let incol = ((xs2 >> 8) & 0x1F) as u64;
        let inrow = ((xs2 >> 13) & 0x3FF) as u64;
        let startcol = ((xs2 >> 23) & 0x1F) as u64;
        let startrow = ((xs2 >> 28) & 0x3FF) as u64;

        if kcol == 0 || krow == 0 || incol == 0 || inrow == 0 {
            return 16;
        }
        if incol < kcol || inrow < krow {
            return 16;
        }

        let row_end = inrow - krow;
        let col_end = incol - kcol;
        if startrow > row_end || startcol > col_end {
            return 16;
        }

        let nwin = (row_end - startrow + 1).saturating_mul(col_end - startcol + 1);
        nwin.saturating_mul(krow).saturating_mul(kcol).max(16)
    }
}
