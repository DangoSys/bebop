use super::super::bank::{BankConfig, BankMap, BANK_NUM};
use super::decode::{rs1_b0, xs2_mset};

pub fn latency(_xs1: u64, _xs2: u64) -> u64 {
    1
}

pub fn exec(xs1: u64, xs2: u64, cfgs: &mut [BankConfig], banks: &mut [Vec<u8>], bank_map: &mut BankMap) -> u64 {
    let bank_id = rs1_b0(xs1);
    let (_, col, alloc) = xs2_mset(xs2);
    if bank_id >= BANK_NUM as u64 {
        panic!("mset: invalid bank_id {bank_id}");
    }
    let v = bank_id as u32;
    let i = bank_id as usize;
    if alloc == 1 {
        bank_map.delete_vbank(v);
        let p = bank_map
            .first_free_pbank()
            .unwrap_or_else(|| panic!("mset: no free physical bank"));
        bank_map.bind(p, v);
        cfgs[i] = BankConfig {
            allocated: true,
            cols: col,
        };
        banks[p].fill(0);
    } else {
        bank_map.delete_vbank(v);
        cfgs[i] = BankConfig {
            allocated: false,
            cols: 0,
        };
    }
    0
}
