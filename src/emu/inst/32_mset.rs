use super::super::bank::{BankConfig, BANK_NUM};
use super::decode::{rs1_b0, xs2_mset};

pub fn exec(xs1: u64, xs2: u64, cfgs: &mut [BankConfig], banks: &mut [Vec<u8>]) -> u64 {
    let bank_id = rs1_b0(xs1);
    let (_, col, alloc) = xs2_mset(xs2);
    if bank_id >= BANK_NUM as u64 {
        panic!("mset: invalid bank_id {bank_id}");
    }
    let i = bank_id as usize;
    if alloc == 1 && cfgs[i].allocated {
        panic!("mset: bank {bank_id} already allocated");
    }
    cfgs[i] = BankConfig {
        allocated: alloc == 1,
        cols: col,
    };
    if alloc == 1 {
        banks[i].fill(0);
    }
    0
}
