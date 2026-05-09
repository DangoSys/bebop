use super::super::bank::{BankConfig, BankMap, BANK_NUM};
use super::bank_matrix::{read_i32_nn, read_i8_nn, write_i32_nn};
use super::decode::{pbank, rs1_b0, rs1_b1, rs1_b2, rs1_iter};
use super::gemmini_state::gemini;

pub fn latency(xs1: u64, _xs2: u64) -> u64 {
    let n = rs1_iter(xs1).max(1).min(64);
    n.saturating_mul(n).saturating_mul(n) / 4 + n.saturating_mul(n)
}

pub fn exec(xs1: u64, _xs2: u64, banks: &mut [Vec<u8>], cfgs: &[BankConfig], bank_map: &BankMap) -> u64 {
    let op_a = rs1_b0(xs1);
    let op_b = rs1_b1(xs1);
    let wr = rs1_b2(xs1);
    let n = rs1_iter(xs1) as usize;
    if op_a >= BANK_NUM as u64 || op_b >= BANK_NUM as u64 || wr >= BANK_NUM as u64 {
        panic!("gemmini_compute_preloaded: invalid bank_id");
    }
    if !cfgs[op_a as usize].allocated || !cfgs[op_b as usize].allocated || !cfgs[wr as usize].allocated {
        panic!("gemmini_compute_preloaded: bank not allocated");
    }
    if n == 0 || n > 64 {
        panic!("gemmini_compute_preloaded: bad iter");
    }

    let pa = pbank(bank_map, op_a);
    let pb = pbank(bank_map, op_b);
    let pw = pbank(bank_map, wr);
    let gm = gemini().lock().unwrap();
    let df = gm.cfg.dataflow;
    let ws_b = gm.ws_b.clone();
    drop(gm);

    if df == 1 {
        let b = ws_b.expect("gemmini_compute_preloaded: WS missing preload");
        let a = read_i8_nn(banks, pa, n);
        let d = read_i32_nn(banks, pb, n);
        let mut c = vec![vec![0i32; n]; n];
        for i in 0..n {
            for j in 0..n {
                let mut acc = d[i][j];
                for k in 0..n {
                    acc += a[i][k] as i32 * b[k][j] as i32;
                }
                c[i][j] = acc;
            }
        }
        write_i32_nn(banks, pw, &c, n);
    } else {
        // OS: C = sum_k a[k][i]*b[k][j] + C_old (A stored as mat_a_t)
        let a = read_i8_nn(banks, pa, n);
        let b = read_i8_nn(banks, pb, n);
        let mut c = read_i32_nn(banks, pw, n);
        for i in 0..n {
            for j in 0..n {
                let mut acc = c[i][j];
                for k in 0..n {
                    acc += a[k][i] as i32 * b[k][j] as i32;
                }
                c[i][j] = acc;
            }
        }
        write_i32_nn(banks, pw, &c, n);
    }
    0
}
