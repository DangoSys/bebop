use super::super::bank::{BankConfig, BANK_NUM, MATRIX_SIZE};
use super::decode::{rs1_b0, rs1_b1, rs1_b2, rs1_iter};

pub fn exec(xs1: u64, xs2: u64, banks: &mut [Vec<u8>], cfgs: &[BankConfig]) -> u64 {
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
    let n = (iter.min(MATRIX_SIZE as u64)) as usize;
    let a_t = read_i8(banks, op1, n);
    let b = read_i8(banks, op2, n);
    let mut c = read_i32(banks, wr, n);
    for i in 0..n {
        for j in 0..n {
            let mut acc = c[i][j];
            for k in 0..n {
                acc = acc.wrapping_add((a_t[k][i] as i32).wrapping_mul(b[k][j] as i32));
            }
            c[i][j] = acc;
        }
    }
    write_i32(banks, wr, &c, n);
    0
}

fn read_i8(banks: &[Vec<u8>], id: u64, n: usize) -> [[i8; MATRIX_SIZE]; MATRIX_SIZE] {
    let mut m = [[0i8; MATRIX_SIZE]; MATRIX_SIZE];
    for i in 0..n {
        for j in 0..n {
            m[i][j] = banks[id as usize][i * 16 + j] as i8;
        }
    }
    m
}

fn read_i32(banks: &[Vec<u8>], id: u64, n: usize) -> [[i32; MATRIX_SIZE]; MATRIX_SIZE] {
    let mut m = [[0i32; MATRIX_SIZE]; MATRIX_SIZE];
    for i in 0..n {
        for j in 0..n {
            let off = i * 64 + j * 4;
            m[i][j] = i32::from_le_bytes(banks[id as usize][off..off + 4].try_into().unwrap());
        }
    }
    m
}

fn write_i32(banks: &mut [Vec<u8>], id: u64, m: &[[i32; MATRIX_SIZE]; MATRIX_SIZE], n: usize) {
    for i in 0..n {
        for j in 0..n {
            let off = i * 64 + j * 4;
            banks[id as usize][off..off + 4].copy_from_slice(&m[i][j].to_le_bytes());
        }
    }
}
