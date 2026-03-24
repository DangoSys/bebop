// Copyright (c) 2026 Buckyball Authors
// SPDX-License-Identifier: Apache-2.0
//! Spike runner entry and process orchestration.
//! It launches the worker and spike, then validates exits.

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use log::{debug, info};

use crate::node;
use crate::shm::{self, ShmMap};
use crate::spike::path::{path_rocc_so, path_system_pk_bin, path_system_spike_bin, SPIKE_EXT};
use crate::utils::path;

static SPIKE_SHM_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn spike_tests(elf: PathBuf, step: bool) -> Result<(), String> {
    let elf = elf.canonicalize().map_err(|e| format!("elf: {e}"))?;
    if !elf.is_file() {
        return Err(format!("not a file: {}", elf.display()));
    }
    let rocc_so = path_rocc_so()?;
    let rocc_dir = rocc_so
        .parent()
        .ok_or("rocc so has no parent")?
        .to_path_buf();
    let spike = path_system_spike_bin()?;
    let pk = path_system_pk_bin()?;
    let ld = rocc_dir.display().to_string();
    run_spike_pk(&spike, &pk, &elf, &ld, step)
}

fn run_spike_pk(
    spike: &PathBuf,
    pk: &PathBuf,
    elf: &Path,
    ld_library_path: &str,
    step: bool,
) -> Result<(), String> {
    let step_mode = if step { "1" } else { "0" };
    let node_file = node::node_file()?;
    let spike_node_id = node::alloc_node_id(&node_file)?;

    // step 1. Generate a unique SHM (Shared Memory) name for the spike process.
    let seq = SPIKE_SHM_SEQ.fetch_add(1, Ordering::Relaxed);
    let shm_name = CString::new(format!("/bebop_spike_{}_{}", std::process::id(), seq))
        .map_err(|_| "shm name has NUL".to_string())?;

    // step 2. Create a new SHM for the spike process.
    let mut map = ShmMap::create_new(&shm_name, shm::BEBOP_SHM_SIZE, false)
        .map_err(|e| format!("shm create: {e}"))?;
    let nm = shm_name
        .to_str()
        .map_err(|_| "shm name is not UTF-8".to_string())?;

    // step 3. Spawn the BEMU worker process.
    // It's a bebop command: bebop bemu-tests
    let bebop_exe = path::path_current_bebop_bin()?;
    let mut worker = Command::new(&bebop_exe)
        .arg("bemu-tests")
        .arg("--node-file")
        .arg(&node_file)
        .env("BEBOP_SHM_NAME", nm)
        .env("BEBOP_STEP", step_mode)
        .spawn()
        .map_err(|e| format!("spawn worker: {e}"))?;
    node::add_child_pid(worker.id() as i32)?;

    // step 4. Spawn the Spike process.
    // It's a spike command:
    // spike <SPIKE_EXT> <PK_BIN> <ELF_FILE> <BEBOP_SHM_NAME> <LD_LIBRARY_PATH> <BEBOP_STEP>
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

    let mut spike_child = Command::new(spike)
        .arg(SPIKE_EXT)
        .arg(pk)
        .arg(elf)
        .env("BEBOP_SHM_NAME", nm)
        .env("LD_LIBRARY_PATH", ld_library_path)
        .env("BEBOP_STEP", step_mode)
        .env("BEBOP_NODE_ID", spike_node_id.to_string())
        .spawn()
        .map_err(|e| {
            let _ = worker.kill();
            let _ = worker.wait();
            let _ = node::remove_child_pid(worker.id() as i32);
            map.set_unlink_on_drop(true);
            format!("spawn spike: {e}")
        })?;
    node::add_child_pid(spike_child.id() as i32)?;
    let spike_st = spike_child.wait().map_err(|e| {
        let _ = worker.kill();
        let _ = worker.wait();
        let _ = node::remove_child_pid(worker.id() as i32);
        let _ = node::remove_child_pid(spike_child.id() as i32);
        map.set_unlink_on_drop(true);
        format!("spike wait: {e}")
    })?;
    let _ = node::remove_child_pid(spike_child.id() as i32);

    // spike exits, then shutdown the BEMU worker.
    // wait for the BEMU worker to exit.
    shm::rpc_shutdown(map.as_bebop());
    let wst = worker.wait().map_err(|e| format!("worker wait: {e}"))?;
    let _ = node::remove_child_pid(worker.id() as i32);
    map.set_unlink_on_drop(true);
    if !wst.success() {
        return Err(format!("worker exited {:?}", wst.code()));
    }

    // spike exits, then return the result.
    if !spike_st.success() {
        return Err(format!("spike exited {:?}", spike_st.code()));
    }

    Ok(())
}
