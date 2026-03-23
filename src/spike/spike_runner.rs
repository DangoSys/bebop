use std::env;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use log::{debug, info};

use crate::shm::{self, ShmMap};

const SPIKE_EXT: &str = "--extension=bebop_rocc";

static SPIKE_SHM_SEQ: AtomicU64 = AtomicU64::new(0);

fn which(cmd: &str) -> Result<PathBuf, String> {
    let out = Command::new("sh")
        .args(["-c", &format!("command -v {cmd}")])
        .output()
        .map_err(|e| format!("which {cmd}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{cmd} not in PATH"));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        return Err(format!("{cmd} not in PATH"));
    }
    Ok(PathBuf::from(s))
}

fn spike_lib_dir(spike_exe: &Path) -> Result<PathBuf, String> {
    let bin = spike_exe
        .parent()
        .ok_or_else(|| "spike: no parent dir".to_string())?;
    let root = bin
        .parent()
        .ok_or_else(|| "spike: install layout has no lib dir".to_string())?;
    Ok(root.join("lib"))
}

fn bebop_crate_root() -> Result<PathBuf, String> {
    let exe = env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let exe = exe
        .canonicalize()
        .map_err(|e| format!("canonicalize exe: {e}"))?;
    let mut dir = exe.parent().ok_or("exe has no parent")?;
    loop {
        if dir.join("src/spike/CMakeLists.txt").is_file() {
            return Ok(dir.to_path_buf());
        }
        dir = dir.parent().ok_or_else(|| {
            format!(
                "bebop root not found (need src/spike/CMakeLists.txt), exe={}",
                exe.display()
            )
        })?;
    }
}

fn path_rocc_so() -> Result<PathBuf, String> {
    let p = bebop_crate_root()?.join("src/spike/build/libbebop_rocc.so");
    if !p.is_file() {
        return Err(format!("missing {}", p.display()));
    }
    p.canonicalize()
        .map_err(|e| format!("canonicalize rocc so: {e}"))
}

fn ld_for_spike(spike_lib: &Path, rocc_dir: &Path) -> String {
    format!("{}:{}", spike_lib.display(), rocc_dir.display())
}

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
    let spike = which("spike")?;
    let pk = which("pk")?;
    let spike_lib = spike_lib_dir(&spike)?;
    let ld = ld_for_spike(&spike_lib, &rocc_dir);
    run_spike_pk(&spike, &pk, &elf, &ld, step)
}

pub fn worker_shm(name: String) -> Result<(), String> {
    let cs = CString::new(name).map_err(|_| "worker-shm: name has NUL")?;
    if !cs.as_bytes().starts_with(b"/") {
        return Err("worker-shm: name must start with '/'".into());
    }
    shm::run_worker(&cs)
}

fn run_spike_pk(
    spike: &Path,
    pk: &Path,
    elf: &Path,
    ld_library_path: &str,
    step: bool,
) -> Result<(), String> {
    let seq = SPIKE_SHM_SEQ.fetch_add(1, Ordering::Relaxed);
    let name = CString::new(format!("/bebop_spike_{}_{}", std::process::id(), seq))
        .map_err(|_| "shm name has NUL")?;

    let mut map = ShmMap::create_new(&name, shm::BEBOP_SHM_SIZE, false)
        .map_err(|e| format!("shm create: {e}"))?;

    let nm = name.to_str().map_err(|_| "shm name is not UTF-8")?;
    let exe = env::current_exe().map_err(|e| format!("current_exe: {e}"))?;

    let mut worker = Command::new(&exe)
        .arg("worker-shm")
        .arg(nm)
        .env("BEBOP_STEP", if step { "1" } else { "0" })
        .spawn()
        .map_err(|e| format!("spawn worker: {e}"))?;

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

    let spike_status = Command::new(spike)
        .arg(SPIKE_EXT)
        .arg(pk)
        .arg(elf)
        .env("BEBOP_SHM_NAME", nm)
        .env("LD_LIBRARY_PATH", ld_library_path)
        .env("BEBOP_STEP", if step { "1" } else { "0" })
        .status();

    let st = match spike_status {
        Ok(s) => s,
        Err(e) => {
            let _ = worker.kill();
            let _ = worker.wait();
            map.set_unlink_on_drop(true);
            return Err(format!("spawn spike: {e}"));
        }
    };

    shm::rpc_shutdown(map.as_bebop());

    let wst = worker.wait().map_err(|e| format!("worker wait: {e}"))?;
    map.set_unlink_on_drop(true);
    if !wst.success() {
        return Err(format!("worker exited {:?}", wst.code()));
    }
    if !st.success() {
        return Err(format!("spike exited {:?}", st.code()));
    }
    Ok(())
}
