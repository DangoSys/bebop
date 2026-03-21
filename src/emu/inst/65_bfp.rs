use super::super::bank::{BankConfig, BANK_NUM};
use super::bank_matrix::{read_i8_nn, write_i32_nn};
use super::decode::{rs1_b0, rs1_b1, rs1_b2, rs1_iter};

/// BFP matmul: same as cpu_matmul — C[i][j] = sum_k A[i][k]*B[k][j].
pub fn exec(xs1: u64, _xs2: u64, banks: &mut [Vec<u8>], cfgs: &[BankConfig]) -> u64 {
    let op1 = rs1_b0(xs1);
    let op2 = rs1_b1(xs1);
    let wr = rs1_b2(xs1);
    let n = rs1_iter(xs1) as usize;
    if op1 >= BANK_NUM as u64 || op2 >= BANK_NUM as u64 || wr >= BANK_NUM as u64 {
        panic!("bfp: invalid bank_id");
    }
    if !cfgs[op1 as usize].allocated
        || !cfgs[op2 as usize].allocated
        || !cfgs[wr as usize].allocated
    {
        panic!("bfp: bank not allocated");
    }
    if cfgs[wr as usize].cols != 4 {
        panic!("bfp: wr bank must be acc (cols=4)");
    }
    if n == 0 || n > 64 {
        panic!("bfp: bad iter");
    }

    let a = read_i8_nn(banks, op1, n);
    let b = read_i8_nn(banks, op2, n);
    let mut c = vec![vec![0i32; n]; n];
    for i in 0..n {
        for j in 0..n {
            let mut acc = 0i32;
            for k in 0..n {
                acc += a[i][k] as i32 * b[k][j] as i32;
            }
            c[i][j] = acc;
        }
    }
    write_i32_nn(banks, wr, &c, n);
    0
}
