use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};

use log::{debug, info};

use crate::framework::node;
use crate::framework::shm::{self, CosimShutdown, ShmMap};
use crate::framework::utils::path;
use crate::node::spike::path::{path_rocc_so, path_system_pk_bin, path_system_spike_bin};
use crate::node::NodeKind;

static SPIKE_SHM_SEQ: AtomicU64 = AtomicU64::new(0);

struct RunningNode {
    kind: NodeKind,
    child: Child,
}

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

pub fn bemu(
    elf: PathBuf,
    step: bool,
    all_banks: bool,
    bemu_config: Option<PathBuf>,
    ipc_stats: bool,
) -> Result<(), String> {
    run(
        elf,
        step,
        all_banks,
        bemu_config,
        WorkerKind::Bemu,
        ipc_stats,
    )
}

#[cfg(all(feature = "verilator", unix))]
pub fn verilator(
    elf: PathBuf,
    step: bool,
    all_banks: bool,
    bemu_config: Option<PathBuf>,
    ipc_stats: bool,
) -> Result<(), String> {
    run(
        elf,
        step,
        all_banks,
        bemu_config,
        WorkerKind::Verilator {
            bank_digest_diff: false,
        },
        ipc_stats,
    )
}

#[cfg(all(feature = "verilator", unix))]
pub fn difftest(
    elf: PathBuf,
    step: bool,
    all_banks: bool,
    bemu_config: Option<PathBuf>,
    ipc_stats: bool,
) -> Result<(), String> {
    run(
        elf,
        step,
        all_banks,
        bemu_config,
        WorkerKind::Verilator {
            bank_digest_diff: true,
        },
        ipc_stats,
    )
}

#[cfg(all(feature = "verilator", not(unix)))]
pub fn verilator(
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

enum WorkerKind {
    Bemu,
    #[cfg(all(feature = "verilator", unix))]
    Verilator {
        bank_digest_diff: bool,
    },
}

fn run(
    elf: PathBuf,
    step: bool,
    all_banks: bool,
    bemu_config: Option<PathBuf>,
    worker: WorkerKind,
    ipc_stats: bool,
) -> Result<(), String> {
    let elf = elf.canonicalize().map_err(|e| format!("elf: {e}"))?;
    if !elf.is_file() {
        return Err(format!("not a file: {}", elf.display()));
    }

    let rocc_so = path_rocc_so()?;
    let spike = path_system_spike_bin()?;
    let pk = path_system_pk_bin()?;
    let ld_library_path = compose_ld_path(&rocc_so, &spike)?;

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

    let (shutdown_mode, dual_cmd, rtl_only, difftest_env, mut worker_nodes): (
        CosimShutdown,
        bool,
        bool,
        &'static str,
        Vec<RunningNode>,
    ) = match worker {
        WorkerKind::Bemu => {
            let mut cmd = Command::new(&bebop_exe);
            cmd.arg("bemu-tests")
                .arg("--node-file")
                .arg(&node_file)
                .arg("--shm-name")
                .arg(nm);
            if !ipc_stats {
                cmd.arg("--no-ipc-stats");
            }
            append_bemu_worker_config(&mut cmd, &bemu_config)?;
            if step {
                cmd.arg("--step");
            }
            if all_banks {
                cmd.arg("--diff-all-banks");
            }
            let child = cmd.spawn().map_err(|e| format!("spawn bemu: {e}"))?;
            node::add_child_pid(child.id() as i32)?;
            (
                CosimShutdown::BemuLane,
                false,
                false,
                "0",
                vec![RunningNode {
                    kind: NodeKind::Bemu,
                    child,
                }],
            )
        }
        #[cfg(all(feature = "verilator", unix))]
        WorkerKind::Verilator { bank_digest_diff } => {
            let difftest_env = if bank_digest_diff { "1" } else { "0" };
            let mut rtl_cmd = Command::new(&bebop_exe);
            rtl_cmd
                .arg("verilator-engine")
                .arg("--node-file")
                .arg(&node_file)
                .arg("--shm-name")
                .arg(nm);
            if !ipc_stats {
                rtl_cmd.arg("--no-ipc-stats");
            }
            if step {
                rtl_cmd.arg("--step");
            }
            if all_banks {
                rtl_cmd.arg("--diff-all-banks");
            }

            if bank_digest_diff {
                let mut bemu_cmd = Command::new(&bebop_exe);
                bemu_cmd
                    .arg("bemu-tests")
                    .arg("--node-file")
                    .arg(&node_file)
                    .arg("--shm-name")
                    .arg(nm);
                if !ipc_stats {
                    bemu_cmd.arg("--no-ipc-stats");
                }
                append_bemu_worker_config(&mut bemu_cmd, &bemu_config)?;
                if step {
                    bemu_cmd.arg("--step");
                }
                if all_banks {
                    bemu_cmd.arg("--diff-all-banks");
                }

                let mut bemu_child = bemu_cmd
                    .spawn()
                    .map_err(|e| format!("spawn bemu node: {e}"))?;
                node::add_child_pid(bemu_child.id() as i32)?;

                let rtl_child = match rtl_cmd.spawn() {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = bemu_child.kill();
                        let _ = bemu_child.wait();
                        let _ = node::remove_child_pid(bemu_child.id() as i32);
                        return Err(format!("spawn verilator node: {e}"));
                    }
                };
                node::add_child_pid(rtl_child.id() as i32)?;

                (
                    CosimShutdown::DualLanes,
                    true,
                    false,
                    difftest_env,
                    vec![
                        RunningNode {
                            kind: NodeKind::Bemu,
                            child: bemu_child,
                        },
                        RunningNode {
                            kind: NodeKind::Verilator,
                            child: rtl_child,
                        },
                    ],
                )
            } else {
                let rtl_child = rtl_cmd
                    .spawn()
                    .map_err(|e| format!("spawn verilator node: {e}"))?;
                node::add_child_pid(rtl_child.id() as i32)?;

                (
                    CosimShutdown::RtlLane,
                    false,
                    true,
                    "0",
                    vec![RunningNode {
                        kind: NodeKind::Verilator,
                        child: rtl_child,
                    }],
                )
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

    let mut spike_cmd = Command::new(&spike);
    spike_cmd
        .arg(&extlib)
        .arg(SPIKE_EXT)
        .arg(&pk)
        .arg(&elf)
        .env("BEBOP_SHM_NAME", nm)
        .env("LD_LIBRARY_PATH", ld_library_path)
        .env("BEBOP_NODE_ID", spike_node_id.to_string())
        .env("BEBOP_DUAL_CMD", if dual_cmd { "1" } else { "0" })
        .env("BEBOP_RTL_ONLY", if rtl_only { "1" } else { "0" })
        .env("BEBOP_DIFFTEST", difftest_env);

    let mut spike_child = spike_cmd.spawn().map_err(|e| {
        for n in &mut worker_nodes {
            let _ = n.child.kill();
            let _ = n.child.wait();
            let _ = node::remove_child_pid(n.child.id() as i32);
        }
        map.set_unlink_on_drop(true);
        format!("spawn {} node: {e}", NodeKind::Spike.as_str())
    })?;
    node::add_child_pid(spike_child.id() as i32)?;

    let spike_st = spike_child.wait().map_err(|e| {
        for n in &mut worker_nodes {
            let _ = n.child.kill();
            let _ = n.child.wait();
            let _ = node::remove_child_pid(n.child.id() as i32);
        }
        let _ = node::remove_child_pid(spike_child.id() as i32);
        map.set_unlink_on_drop(true);
        format!("{} node wait: {e}", NodeKind::Spike.as_str())
    })?;
    let _ = node::remove_child_pid(spike_child.id() as i32);

    shm::rpc_shutdown(map.as_bebop(), shutdown_mode);

    for n in &mut worker_nodes {
        let st = n
            .child
            .wait()
            .map_err(|e| format!("{} node wait: {e}", n.kind.as_str()))?;
        let _ = node::remove_child_pid(n.child.id() as i32);
        if !st.success() {
            map.set_unlink_on_drop(true);
            return Err(format!("{} node exited {:?}", n.kind.as_str(), st.code()));
        }
    }

    map.set_unlink_on_drop(true);
    if !spike_st.success() {
        return Err(format!(
            "{} node exited {:?}",
            NodeKind::Spike.as_str(),
            spike_st.code()
        ));
    }

    Ok(())
}
