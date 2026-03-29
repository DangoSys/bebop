//! BEMU worker plus Verilator cosim step per handled RoCC insn.

use std::env;
use std::ffi::CString;

use crate::node;
use crate::shm::layout::BEBOP_SHM_SIZE;
use crate::shm::protocol::OpReq;
use crate::shm::ShmMap;
use crate::verilator::{cosim_issue, cosim_result, CosimGuard};

use super::bemu::{Bemu, StepCfg};
use super::diff::config::DiffCfg;
use super::runner::{run_cmd, Tick};

pub fn vl_worker_tests(step_on: bool, diff_all_banks: bool) -> Result<(), String> {
    let node_id = node::node_id();
    if node_id == 0 {
        return Err("node_id must be > 0".to_string());
    }
    let name = env::var("BEBOP_SHM_NAME").map_err(|_| "missing env BEBOP_SHM_NAME".to_string())?;
    let cs = CString::new(name).map_err(|_| "verilator-worker: name has NUL")?;
    if !cs.as_bytes().starts_with(b"/") {
        return Err("verilator-worker: name must start with '/'".into());
    }
    let map = ShmMap::attach(cs.as_c_str(), BEBOP_SHM_SIZE)
        .map_err(|e| format!("worker shm attach: {e}"))?;
    let shm = map.raw_bebop();
    let mut bemu = Bemu::new();
    let mut step = StepCfg {
        on: step_on,
        idx: 0,
    };
    let diff = DiffCfg {
        all_banks: diff_all_banks,
    };
    let _cosim = CosimGuard::new();

    loop {
        unsafe {
            match run_cmd(
                shm,
                node_id,
                &mut bemu,
                &mut step,
                &diff,
                &mut |req, resp| {
                    if let OpReq::CmdHandle { funct, xs1, xs2 } = req {
                        if resp.err == 0 && !resp.done {
                            cosim_issue(funct, xs1, xs2);
                            let rtl = cosim_result();
                            let gold = resp.result.unwrap_or_else(|| {
                                panic!("BEMU returned no result for CmdHandle (funct={funct})");
                            });
                            if rtl != gold {
                                panic!(
                                    "RTL vs BEMU result mismatch: funct={funct} xs1=0x{xs1:x} xs2=0x{xs2:x} bemu=0x{gold:x} rtl=0x{rtl:x}"
                                );
                            }
                        }
                    }
                },
            ) {
                Tick::Done => return Ok(()),
                Tick::Worked => {}
                Tick::Idle => std::thread::yield_now(),
            }
        }
    }
}
