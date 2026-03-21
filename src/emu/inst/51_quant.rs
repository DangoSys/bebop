use super::super::bank::{BankConfig, BankMap, BANK_NUM, BANK_SIZE};
use super::decode::{pbank, rs1_b0, rs1_b2, rs1_iter};

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
        panic!("quant: invalid bank_id");
    }
    let sc = cfgs[src as usize];
    let dc = cfgs[dst as usize];
    if !sc.allocated || !dc.allocated {
        panic!("quant: bank not allocated");
    }
    if sc.cols != 4 || dc.cols != 1 {
        panic!(
            "quant: unsupported layout src_cols={} dst_cols={}",
            sc.cols, dc.cols
        );
    }
    let ps = pbank(bank_map, src);
    let pd = pbank(bank_map, dst);
    let scale = f32::from_bits((xs2 & 0xffff_ffff) as u32);
    for i in 0..depth {
        let src_base = i * 64;
        let dst_base = i * 16;
        if src_base + 64 > BANK_SIZE || dst_base + 16 > BANK_SIZE {
            panic!("quant: out of range");
        }
        for j in 0..16 {
            let off = src_base + j * 4;
            let v = i32::from_le_bytes(banks[ps][off..off + 4].try_into().unwrap());
            let q = ((v as f32) * scale).round().clamp(-128.0, 127.0) as i8;
            banks[pd][dst_base + j] = q as u8;
        }
    }
    0
}
