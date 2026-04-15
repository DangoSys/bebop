//! Verilator RTL process: `cmd_rtl` + `mem_rtl` (`bebop verilator` alone, or with `bemu-tests` for `difftest`).

use std::ffi::CString;
use std::time::Instant;

use crate::framework::node;
use crate::framework::shm::layout::{BebopShm, BEBOP_SHM_SIZE};
use crate::framework::shm::ShmMap;
use crate::framework::utils::ipc_stats;
use crate::node::verilator::{cosim_set_mem16_reader, cosim_set_mem16_writer, CosimGuard};

use super::diff::config::DiffCfg;
use super::runner::{mem_req_write16, run_cmd_rtl, shm_mem_read16, ShmMemLane, Tick};

pub fn run(
    step_on: bool,
    diff_all_banks: bool,
    shm_name: String,
    ipc_stats_on: bool,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    unsafe {
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
    }
    let node_id = node::node_id();
    if node_id == 0 {
        return Err("verilator-engine: node_id must be > 0".into());
    }
    ipc_stats::set_on(ipc_stats_on);
    let cs = CString::new(shm_name).map_err(|_| "verilator-engine: shm name has NUL")?;
    if !cs.as_bytes().starts_with(b"/") {
        return Err("verilator-engine: shm name must start with '/'".into());
    }

    let map = ShmMap::attach(cs.as_c_str(), BEBOP_SHM_SIZE)
        .map_err(|e| format!("verilator-engine: shm attach: {e}"))?;
    let shm = map.raw_bebop();
    let shm_usize = shm as usize;
    cosim_set_mem16_reader(move |addr| unsafe {
        shm_mem_read16(shm_usize as *mut BebopShm, ShmMemLane::Rtl, node_id, addr)
    });
    cosim_set_mem16_writer(move |addr, data| unsafe {
        mem_req_write16(
            shm_usize as *mut BebopShm,
            ShmMemLane::Rtl,
            node_id,
            addr,
            data,
        );
    });
    let _cosim = CosimGuard::new();

    let diff = DiffCfg {
        all_banks: diff_all_banks,
    };

    loop {
        let t_iter = Instant::now();
        unsafe {
            match run_cmd_rtl(shm, node_id, &diff, step_on) {
                Tick::Done => {
                    ipc_stats::worker_work(t_iter.elapsed());
                    ipc_stats::eprint_worker_summary("verilator-engine");
                    return Ok(());
                }
                Tick::Worked => ipc_stats::worker_work(t_iter.elapsed()),
                Tick::Idle => {
                    ipc_stats::worker_idle(t_iter.elapsed());
                    std::thread::yield_now();
                }
            }
        }
    }
}
