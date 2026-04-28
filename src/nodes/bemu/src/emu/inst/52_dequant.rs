use super::super::bank::{BankConfig, BankMap, BANK_NUM, BANK_SIZE};
use super::decode::{pbank, rs1_b0, rs1_b2, rs1_iter};

pub fn latency(xs1: u64, _xs2: u64) -> u64 {
    rs1_iter(xs1).max(1)
}

pub fn exec(
    xs1: u64,
    xs2: u64,
    banks: &mut [Vec<u8>],
    cfgs: &[BankConfig],
    bank_map: &BankMap,
) -> u64 {
    let src = rs1_b0(xs1);
    let dst = rs1_b2(xs1);
    let depth = rs1_iter(xs1) as usize;
    if src >= BANK_NUM as u64 || dst >= BANK_NUM as u64 {
        panic!("dequant: invalid bank_id");
    }
    let sc = cfgs[src as usize];
    let dc = cfgs[dst as usize];
    if !sc.allocated || !dc.allocated {
        panic!("dequant: bank not allocated");
    }
    if sc.cols != 1 || dc.cols != 4 {
        panic!(
            "dequant: unsupported layout src_cols={} dst_cols={}",
            sc.cols, dc.cols
        );
    }
    let ps = pbank(bank_map, src);
    let pd = pbank(bank_map, dst);
    let scale = f32::from_bits((xs2 & 0xffff_ffff) as u32);
    for i in 0..depth {
        let src_base = i * 16;
        let dst_base = i * 64;
        if src_base + 16 > BANK_SIZE || dst_base + 64 > BANK_SIZE {
            panic!("dequant: out of range");
        }
        for j in 0..16 {
            let v = banks[ps][src_base + j] as i8;
            let o = ((v as f32) * scale).round() as i32;
            let off = dst_base + j * 4;
            banks[pd][off..off + 4].copy_from_slice(&o.to_le_bytes());
        }
    }
    0
}
