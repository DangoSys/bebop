use super::super::bank::{BankConfig, BankMap, BANK_NUM};
use super::bank_matrix::{read_i32_16x16, read_i8_k_rows, write_i32_16x16};
use super::decode::{pbank, rs1_b0, rs1_b1, rs1_b2, rs1_iter};

/// Output tile M×N; inner sum length K = iter (VecBall mesh is 16-wide).
const WARP_M: usize = 16;
const WARP_N: usize = 16;

pub fn exec(
    xs1: u64,
    xs2: u64,
    banks: &mut [Vec<u8>],
    cfgs: &[BankConfig],
    bank_map: &BankMap,
) -> u64 {
    let op1 = rs1_b0(xs1);
    let op2 = rs1_b1(xs1);
    let wr = rs1_b2(xs1);
    let iter = rs1_iter(xs1);
    let _ = xs2;
    if op1 >= BANK_NUM as u64 || op2 >= BANK_NUM as u64 || wr >= BANK_NUM as u64 {
        panic!("mul_warp16: invalid bank_id");
    }
    let c1 = cfgs[op1 as usize].cols;
    let c2 = cfgs[op2 as usize].cols;
    let cw = cfgs[wr as usize].cols;
    if c1 != 1 || c2 != 1 || cw != 4 {
        panic!("mul_warp16: unsupported bank layout op1_cols={c1} op2_cols={c2} wr_cols={cw}");
    }
    let p1 = pbank(bank_map, op1);
    let p2 = pbank(bank_map, op2);
    let pw = pbank(bank_map, wr);
    let kin = iter as usize;
    if kin == 0 {
        panic!("mul_warp16: iter must be > 0");
    }
    let need = kin * 16;
    if need > banks[p1].len() || need > banks[p2].len() {
        panic!("mul_warp16: iter too large for bank");
    }
    let a_t = read_i8_k_rows(banks, p1, kin, WARP_N);
    let b = read_i8_k_rows(banks, p2, kin, WARP_N);
    let mut c = read_i32_16x16(banks, pw);
    for i in 0..WARP_M {
        for j in 0..WARP_N {
            let mut acc = c[i][j];
            for t in 0..kin {
                acc = acc.wrapping_add((a_t[t][i] as i32).wrapping_mul(b[t][j] as i32));
            }
            c[i][j] = acc;
        }
    }
    write_i32_16x16(banks, pw, &c);
    0
}
