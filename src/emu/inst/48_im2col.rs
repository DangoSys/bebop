use super::super::bank::{BankConfig, BANK_NUM};
use super::decode::{rs1_b0, rs1_b2};

/// Row-major input A[M][K] in bank (after mvin), output flattened im2col windows to wr bank.
pub fn exec(xs1: u64, xs2: u64, banks: &mut [Vec<u8>], cfgs: &[BankConfig]) -> u64 {
    let op1 = rs1_b0(xs1);
    let wr = rs1_b2(xs1);
    if op1 >= BANK_NUM as u64 || wr >= BANK_NUM as u64 {
        panic!("im2col: invalid bank_id");
    }
    if !cfgs[op1 as usize].allocated || !cfgs[wr as usize].allocated {
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

    let oi = op1 as usize;
    let wi = wr as usize;
    let (srcb, dstb): (&[u8], &mut [u8]) = if oi < wi {
        let (l, r) = banks.split_at_mut(wi);
        (&l[oi], &mut r[0])
    } else {
        let (l, r) = banks.split_at_mut(oi);
        (&r[0], &mut l[wi])
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
