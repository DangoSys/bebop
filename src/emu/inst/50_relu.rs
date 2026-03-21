use super::super::bank::{BankConfig, BANK_NUM, BANK_SIZE};
use super::decode::{rs1_b0, rs1_b2, rs1_iter};

pub fn exec(xs1: u64, banks: &mut [Vec<u8>], cfgs: &[BankConfig]) -> u64 {
    let src = rs1_b0(xs1);
    let dst = rs1_b2(xs1);
    let depth = rs1_iter(xs1) as usize;
    if src >= BANK_NUM as u64 || dst >= BANK_NUM as u64 {
        panic!("relu: invalid bank_id");
    }
    let sc = cfgs[src as usize];
    let dc = cfgs[dst as usize];
    if !sc.allocated || !dc.allocated {
        panic!("relu: bank not allocated");
    }
    if sc.cols == 1 && dc.cols == 1 {
        for i in 0..depth {
            let base = i * 16;
            if base + 16 > BANK_SIZE {
                panic!("relu: out of range");
            }
            for j in 0..16 {
                let v = banks[src as usize][base + j] as i8;
                banks[dst as usize][base + j] = if v < 0 { 0 } else { v as u8 };
            }
        }
        return 0;
    }
    if sc.cols == 4 && dc.cols == 4 {
        for i in 0..depth {
            let base = i * 64;
            if base + 64 > BANK_SIZE {
                panic!("relu: out of range");
            }
            for j in 0..16 {
                let off = base + j * 4;
                let v = i32::from_le_bytes(banks[src as usize][off..off + 4].try_into().unwrap());
                let o = if v < 0 { 0 } else { v };
                banks[dst as usize][off..off + 4].copy_from_slice(&o.to_le_bytes());
            }
        }
        return 0;
    }
    panic!(
        "relu: unsupported layout src_cols={} dst_cols={}",
        sc.cols, dc.cols
    );
}
