// Copyright (c) 2026 Buckyball Authors
// SPDX-License-Identifier: Apache-2.0
//! Spike runner entry and process orchestration.
//! It launches the worker and spike, then validates exits.

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};

use log::{debug, info};

use crate::framework::node;
use crate::framework::shm::{self, CosimShutdown, ShmMap};
use crate::framework::utils::path;
use crate::node::spike::path::{path_rocc_so, path_system_pk_bin, path_system_spike_bin};

static SPIKE_SHM_SEQ: AtomicU64 = AtomicU64::new(0);

fn append_bemu_worker_config(cmd: &mut Command, config: &Option<PathBuf>) -> Result<(), String> {
    if let Some(p) = config {
        let p = p.canonicalize().map_err(|e| format!("--config: {e}"))?;
        if !p.is_file() {
            return Err(format!("--config is not a file: {}", p.display()));
        }
        cmd.arg("--config").arg(p);
    }
    Ok(())
}

fn compose_ld_path(rocc_so: &Path, spike_bin: &Path) -> Result<String, String> {
    let rocc_dir = rocc_so
        .parent()
        .ok_or("rocc so has no parent")?
        .to_path_buf();
    let spike_lib = spike_bin
        .parent()
        .ok_or("spike bin has no parent")?
        .join("../lib");
    if spike_lib.is_dir() {
        return Ok(format!("{}:{}", rocc_dir.display(), spike_lib.display()));
    }
    Ok(rocc_dir.display().to_string())
}

pub fn spike_tests(
    elf: PathBuf,
    step: bool,
    all_banks: bool,
    bemu_config: Option<PathBuf>,
    ipc_stats: bool,
) -> Result<(), String> {
    let elf = elf.canonicalize().map_err(|e| format!("elf: {e}"))?;
    if !elf.is_file() {
        return Err(format!("not a file: {}", elf.display()));
    }
    let rocc_so = path_rocc_so()?;
    let spike = path_system_spike_bin()?;
    let pk = path_system_pk_bin()?;
    let ld = compose_ld_path(&rocc_so, &spike)?;
    run_spike_pk(
        &spike,
        &pk,
        &elf,
        &rocc_so,
        &ld,
        step,
        all_banks,
        &bemu_config,
        WorkerKind::Bemu,
        ipc_stats,
    )
}

/// Spike + `verilator-engine` only: RTL lane (`cmd_rtl` / `mem_rtl`), no `bemu-tests`.
#[cfg(all(feature = "verilator", unix))]
pub fn verilator_tests(
    elf: PathBuf,
    step: bool,
    all_banks: bool,
    bemu_config: Option<PathBuf>,
    ipc_stats: bool,
) -> Result<(), String> {
    run_verilator_elf(elf, step, all_banks, false, bemu_config, ipc_stats)
}

/// `bemu-tests` + `verilator-engine`: dual lane; `rd` must match; optional **FNV bank_digest** (`BEBOP_DIFFTEST`).
#[cfg(all(feature = "verilator", unix))]
pub fn difftest(
    elf: PathBuf,
    step: bool,
    all_banks: bool,
    bemu_config: Option<PathBuf>,
    ipc_stats: bool,
) -> Result<(), String> {
    run_verilator_elf(elf, step, all_banks, true, bemu_config, ipc_stats)
}

#[cfg(all(feature = "verilator", not(unix)))]
pub fn verilator_tests(
    _elf: PathBuf,
    _step: bool,
    _all_banks: bool,
    _bemu_config: Option<PathBuf>,
    _ipc_stats: bool,
) -> Result<(), String> {
    Err("verilator cosim requires Unix".into())
}

#[cfg(all(feature = "verilator", not(unix)))]
pub fn difftest(
    _elf: PathBuf,
    _step: bool,
    _all_banks: bool,
    _bemu_config: Option<PathBuf>,
    _ipc_stats: bool,
) -> Result<(), String> {
    Err("verilator cosim requires Unix".into())
}

#[cfg(all(feature = "verilator", unix))]
fn run_verilator_elf(
    elf: PathBuf,
    step: bool,
    all_banks: bool,
    bank_digest_diff: bool,
    bemu_config: Option<PathBuf>,
    ipc_stats: bool,
) -> Result<(), String> {
    let elf = elf.canonicalize().map_err(|e| format!("elf: {e}"))?;
    if !elf.is_file() {
        return Err(format!("not a file: {}", elf.display()));
    }
    let rocc_so = path_rocc_so()?;
    let spike = path_system_spike_bin()?;
    let pk = path_system_pk_bin()?;
    let ld = compose_ld_path(&rocc_so, &spike)?;
    run_spike_pk(
        &spike,
        &pk,
        &elf,
        &rocc_so,
        &ld,
        step,
        all_banks,
        &bemu_config,
        WorkerKind::Verilator { bank_digest_diff },
        ipc_stats,
    )
}

enum WorkerKind {
    Bemu,
    #[cfg(all(feature = "verilator", unix))]
    Verilator {
        bank_digest_diff: bool,
    },
}

fn run_spike_pk(
    spike: &PathBuf,
    pk: &PathBuf,
    elf: &Path,
    rocc_so: &Path,
    ld_library_path: &str,
    step: bool,
    all_banks: bool,
    bemu_config: &Option<PathBuf>,
    worker: WorkerKind,
    ipc_stats: bool,
) -> Result<(), String> {
    if ipc_stats {
        std::env::set_var("BEBOP_IPC_STATS", "1");
    } else {
        std::env::set_var("BEBOP_IPC_STATS", "0");
    }

    const SPIKE_EXT: &str = "--extension=bebop_rocc";
    let extlib = format!("--extlib={}", rocc_so.display());
    let node_file = node::node_file()?;
    let spike_node_id = node::alloc_node_id(&node_file)?;

    let seq = SPIKE_SHM_SEQ.fetch_add(1, Ordering::Relaxed);
    let shm_name = CString::new(format!("/bebop_spike_{}_{}", std::process::id(), seq))
        .map_err(|_| "shm name has NUL".to_string())?;

    let mut map = ShmMap::create_new(&shm_name, shm::BEBOP_SHM_SIZE, false)
        .map_err(|e| format!("shm create: {e}"))?;
    let nm = shm_name
        .to_str()
        .map_err(|_| "shm name is not UTF-8".to_string())?;

    let bebop_exe = path::path_current_bebop_bin()?;

    let (shutdown_mode, dual_cmd, rtl_only, difftest_env, mut children): (
        CosimShutdown,
        bool,
        bool,
        &'static str,
        Vec<Child>,
    ) = match worker {
        WorkerKind::Bemu => {
            let mut c = Command::new(&bebop_exe);
            c.arg("bemu-tests")
                .arg("--node-file")
                .arg(&node_file)
                .env("BEBOP_SHM_NAME", nm);
            append_bemu_worker_config(&mut c, bemu_config)?;
            if step {
                c.arg("--step");
            }
            if all_banks {
                c.arg("--diff-all-banks");
            }
            let w = c.spawn().map_err(|e| format!("spawn worker: {e}"))?;
            node::add_child_pid(w.id() as i32)?;
            (CosimShutdown::BemuLane, false, false, "0", vec![w])
        }
        #[cfg(all(feature = "verilator", unix))]
        WorkerKind::Verilator { bank_digest_diff } => {
            let difftest_env = if bank_digest_diff { "1" } else { "0" };
            let mut r = Command::new(&bebop_exe);
            r.arg("verilator-engine")
                .arg("--node-file")
                .arg(&node_file)
                .env("BEBOP_SHM_NAME", nm);
            if step {
                r.arg("--step");
            }
            if all_banks {
                r.arg("--diff-all-banks");
            }
            if bank_digest_diff {
                let mut b = Command::new(&bebop_exe);
                b.arg("bemu-tests")
                    .arg("--node-file")
                    .arg(&node_file)
                    .env("BEBOP_SHM_NAME", nm);
                append_bemu_worker_config(&mut b, bemu_config)?;
                if step {
                    b.arg("--step");
                }
                if all_banks {
                    b.arg("--diff-all-banks");
                }
                let mut wb = b.spawn().map_err(|e| format!("spawn bemu-tests: {e}"))?;
                node::add_child_pid(wb.id() as i32)?;
                let wr = r.spawn().map_err(|e| {
                    let _ = wb.kill();
                    let _ = wb.wait();
                    let _ = node::remove_child_pid(wb.id() as i32);
                    format!("spawn verilator-engine: {e}")
                })?;
                node::add_child_pid(wr.id() as i32)?;
                (
                    CosimShutdown::DualLanes,
                    true,
                    false,
                    difftest_env,
                    vec![wb, wr],
                )
            } else {
                let wr = r
                    .spawn()
                    .map_err(|e| format!("spawn verilator-engine: {e}"))?;
                node::add_child_pid(wr.id() as i32)?;
                (CosimShutdown::RtlLane, false, true, "0", vec![wr])
            }
        }
    };

    debug!(
        "LD_LIBRARY_PATH={} {} {} {} {} BEBOP_SHM_NAME={}",
        ld_library_path,
        spike.display(),
        SPIKE_EXT,
        pk.display(),
        elf.display(),
        nm
    );
    info!("spike: {}", elf.display());

    let mut spike_cmd = Command::new(spike);
    spike_cmd
        .arg(&extlib)
        .arg(SPIKE_EXT)
        .arg(pk)
        .arg(elf)
        .env("BEBOP_SHM_NAME", nm)
        .env("LD_LIBRARY_PATH", ld_library_path)
        .env("BEBOP_NODE_ID", spike_node_id.to_string())
        .env("BEBOP_DUAL_CMD", if dual_cmd { "1" } else { "0" })
        .env("BEBOP_RTL_ONLY", if rtl_only { "1" } else { "0" })
        .env("BEBOP_DIFFTEST", difftest_env);

    let mut spike_child = spike_cmd.spawn().map_err(|e| {
        for w in &mut children {
            let _ = w.kill();
            let _ = w.wait();
            let _ = node::remove_child_pid(w.id() as i32);
        }
        map.set_unlink_on_drop(true);
        format!("spawn spike: {e}")
    })?;
    node::add_child_pid(spike_child.id() as i32)?;

    let spike_st = spike_child.wait().map_err(|e| {
        for w in &mut children {
            let _ = w.kill();
            let _ = w.wait();
            let _ = node::remove_child_pid(w.id() as i32);
        }
        let _ = node::remove_child_pid(spike_child.id() as i32);
        map.set_unlink_on_drop(true);
        format!("spike wait: {e}")
    })?;
    let _ = node::remove_child_pid(spike_child.id() as i32);

    shm::rpc_shutdown(map.as_bebop(), shutdown_mode);

    for mut w in children {
        let wst = w.wait().map_err(|e| format!("worker wait: {e}"))?;
        let _ = node::remove_child_pid(w.id() as i32);
        if !wst.success() {
            map.set_unlink_on_drop(true);
            return Err(format!("worker exited {:?}", wst.code()));
        }
    }

    map.set_unlink_on_drop(true);
    if !spike_st.success() {
        return Err(format!("spike exited {:?}", spike_st.code()));
    }

    Ok(())
}
