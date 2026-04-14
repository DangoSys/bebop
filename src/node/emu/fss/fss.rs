use crate::framework::shm::protocol::OpResp;
use crate::node::emu::bank::{BankConfig, BankMap};
use crate::node::emu::bemu::StepCfg;
use crate::node::emu::diff::config::DiffCfg;
use crate::node::emu::inst::decode;
use crate::node::emu::inst::exec_latency;

const MEM_BLK: usize = 16;

/// FSS: same functional semantics as ISS, plus cumulative cycle estimate (issue→complete heuristics).
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
    _step: &mut StepCfg,
    _diff: &DiffCfg,
    latency: &mut u64,
) -> OpResp {
    //===----------------------------------------------------------------------===//
    //
    // Under FSS (Function Set Simulator) Mode, we simulate the entire function.
    // All the instructions are simulated with latency.
    //
    //===----------------------------------------------------------------------===//
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

    let cy = exec_latency::inst_cycles(funct, xs1, xs2);
    *latency = latency
        .checked_add(cy)
        .unwrap_or_else(|| panic!("FSS latency overflow (acc={latency}, +{cy})"));

    log::info!("FSS: latency={latency}");

    let mut resp = OpResp::ok();
    resp.result = Some(out);
    resp
}
