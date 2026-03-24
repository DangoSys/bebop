use super::super::bank::{BankConfig, BankMap, BANK_NUM};
use super::bank_matrix::{read_i32_16x16, write_i32_16x16};
use super::decode::{pbank, rs1_b0, rs1_b1, rs1_b2, rs1_iter};

pub fn latency(xs1: u64, _xs2: u64) -> u64 {
    let kin = rs1_iter(xs1).max(1);
    kin.saturating_mul(16)
}

/// op1: Aᵀ row-major; op2: B row-major. C[i,j]=Σ_k A[i,k]B[k,j]; K=iter.
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
    if kin == 0 || kin % 16 != 0 {
        panic!("mul_warp16: iter must be non-zero and multiple of 16");
    }
    let need_b = kin * 16;
    let need_a = kin * 16;
    if need_a > banks[p1].len() || need_b > banks[p2].len() {
        panic!("mul_warp16: iter too large for bank");
    }
    let a_mem = &banks[p1];
    let b_mem = &banks[p2];
    let mut c = read_i32_16x16(banks, pw);
    for i in 0..WARP_M {
        for j in 0..WARP_N {
            let mut acc = c[i][j];
            for k in 0..kin {
                let a_off = k * 16 + i;
                let b_off = k * 16 + j;
                let a = a_mem[a_off] as i8 as i32;
                let b = b_mem[b_off] as i8 as i32;
                acc = acc.wrapping_add(a.wrapping_mul(b));
            }
            c[i][j] = acc;
        }
    }
    write_i32_16x16(banks, pw, &c);
    0
}
