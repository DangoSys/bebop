use super::super::bank::{BankConfig, BANK_NUM, MATRIX_SIZE};
use super::decode::{rs1_b0, rs1_b2, rs1_iter};

/// Row-major A[M][K] (M=16 lanes) after mvin → Aᵀ[K][M] row-major at dst.
const TRANSPOSE_M: usize = 16;

pub fn exec(xs1: u64, xs2: u64, banks: &mut [Vec<u8>], cfgs: &[BankConfig]) -> u64 {
    let op1 = rs1_b0(xs1);
    let wr = rs1_b2(xs1);
    let iter = rs1_iter(xs1);
    let _ = xs2;
    if op1 >= BANK_NUM as u64 || wr >= BANK_NUM as u64 {
        panic!("transpose: invalid bank_id");
    }
    let c1 = cfgs[op1 as usize].cols;
    let cw = cfgs[wr as usize].cols;
    let k = iter as usize;
    if c1 == 1 && cw == 1 {
        if k == 0 {
            panic!("transpose: iter must be > 0");
        }
        let oi = op1 as usize;
        let wi = wr as usize;
        if oi == wi {
            panic!("transpose: op1 and wr must differ");
        }
        let (srcb, dstb): (&[u8], &mut [u8]) = if oi < wi {
            let (l, r) = banks.split_at_mut(wi);
            (&l[oi], &mut r[0])
        } else {
            let (l, r) = banks.split_at_mut(oi);
            (&r[0], &mut l[wi])
        };
        for r in 0..TRANSPOSE_M {
            for c in 0..k {
                let src = r * k + c;
                let dst = c * TRANSPOSE_M + r;
                if src >= srcb.len() || dst >= dstb.len() {
                    panic!("transpose: bank range src={src} dst={dst}");
                }
                dstb[dst] = srcb[src];
            }
        }
        return 0;
    }
    let n = (iter.min(MATRIX_SIZE as u64)) as usize;
    if c1 == 4 && cw == 4 {
        for i in 0..n {
            for j in 0..n {
                let src_off = i * 64 + j * 4;
                let dst_off = j * 64 + i * 4;
                let v = i32::from_le_bytes(
                    banks[op1 as usize][src_off..src_off + 4]
                        .try_into()
                        .unwrap(),
                );
                banks[wr as usize][dst_off..dst_off + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        return 0;
    }
    panic!("transpose: unsupported bank layout op1_cols={c1} wr_cols={cw}");
}
