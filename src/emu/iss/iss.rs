use crate::emu::bank::{BankConfig, BankMap};
use crate::emu::bemu::StepCfg;
use crate::emu::diff::config::DiffCfg;
use crate::emu::diff::hash::bank_hash;
use crate::emu::inst::decode;
use crate::shm::protocol::OpResp;

const MEM_BLK: usize = 16;

pub fn execute_inst(
    funct: u32,
    xs1: u64,
    xs2: u64,
    memory: &mut [u8],
    mem_read16: &mut dyn FnMut(u64) -> [u8; MEM_BLK],
    mem_write16: &mut dyn FnMut(u64, [u8; MEM_BLK]),
    banks: &mut [Vec<u8>],
    bank_cfg: &mut [BankConfig],
    bank_map: &mut BankMap,
    step: &mut StepCfg,
    diff: &DiffCfg,
) -> OpResp {
    let out = match decode::execute_known(
        funct,
        xs1,
        xs2,
        memory,
        mem_read16,
        mem_write16,
        banks,
        bank_cfg,
        bank_map,
    ) {
        Some(v) => {
            if v == 0 {
                funct as u64
            } else {
                0
            }
        }
        None => panic!("Bemu: unknown funct={funct}"),
    };
    if step.on {
        step.idx = step.idx.wrapping_add(1);
        let bank_s = bank_hash(banks, bank_cfg, diff.all_banks);
        println!(
            "step={} funct={} xs1=0x{:x} xs2=0x{:x} out=0x{:x} {}",
            step.idx, funct, xs1, xs2, out, bank_s
        );
    }
    let mut resp = OpResp::ok();
    resp.result = Some(out);
    resp
}
