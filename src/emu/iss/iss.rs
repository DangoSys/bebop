use crate::emu::bank::{BankConfig, BankMap};
use crate::emu::inst::decode;

pub fn execute_inst(
    funct: u32,
    xs1: u64,
    xs2: u64,
    memory: &mut [u8],
    banks: &mut [Vec<u8>],
    bank_cfg: &mut [BankConfig],
    bank_map: &mut BankMap,
) -> u64 {
    match decode::execute_known(funct, xs1, xs2, memory, banks, bank_cfg, bank_map) {
        Some(v) => encode_result(funct, v),
        None => panic!("Bemu: unknown funct={funct}"),
    }
}

fn encode_result(funct: u32, ret: u64) -> u64 {
    if ret == 0 {
        funct as u64
    } else {
        0
    }
}
