use std::env;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use log::{debug, info};

use crate::shm::{self, ShmMap};

const SPIKE_EXT: &str = "--extension=bebop_rocc";

static SPIKE_SHM_SEQ: AtomicU64 = AtomicU64::new(0);

fn resolve_on_path(cmd: &str) -> Result<PathBuf, String> {
    let out = Command::new("sh")
        .args(["-c", &format!("command -v {cmd}")])
        .output()
        .map_err(|e| format!("failed to run sh for command -v {cmd}: {e}"))?;
    if !out.status.success() {
        return Err(format!("'{cmd}' not found in PATH (run `nix develop`?)"));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        return Err(format!("'{cmd}' not found in PATH"));
    }
    Ok(PathBuf::from(s))
}

fn spike_lib_dir(spike_exe: &Path) -> Result<PathBuf, String> {
    let bin_dir = spike_exe
        .parent()
        .ok_or_else(|| "spike path has no parent".to_string())?;
    let root = bin_dir
        .parent()
        .ok_or_else(|| "spike install layout unexpected".to_string())?;
    Ok(root.join("lib"))
}

fn bebop_crate_root() -> Result<PathBuf, String> {
    let exe = env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let exe = exe.canonicalize().map_err(|e| format!("bebop exe: {e}"))?;
    let mut dir = exe.parent().ok_or("bebop exe has no parent")?;
    loop {
        if dir.join("src/workload/CMakeLists.txt").is_file() {
            return Ok(dir.to_path_buf());
        }
        dir = dir.parent().ok_or_else(|| {
            format!(
                "bebop crate root not found (need src/workload/CMakeLists.txt) starting from {}",
                exe.display()
            )
        })?;
    }
}

fn path_rocc_so() -> Result<PathBuf, String> {
    let p = bebop_crate_root()?.join("src/workload/build/libbebop_rocc.so");
    if !p.is_file() {
        return Err(format!("missing {} — run `bebop build`", p.display()));
    }
    p.canonicalize().map_err(|e| format!("rocc path: {e}"))
}

pub fn spike_tests(elf: PathBuf, step: bool) -> Result<(), String> {
    let elf = elf.canonicalize().map_err(|e| format!("elf path: {e}"))?;
    if !elf.is_file() {
        return Err(format!("elf is not a file: {}", elf.display()));
    }
    let rocc_so = path_rocc_so()?;
    let rocc_dir = rocc_so
        .parent()
        .ok_or("rocc path has no parent")?
        .to_path_buf();
    let spike = resolve_on_path("spike")?;
    let pk = resolve_on_path("pk")?;
    let spike_lib = spike_lib_dir(&spike)?;
    let ld = ld_library_path_spike(&spike_lib, &rocc_dir);
    run_spike_pk(&spike, &pk, &elf, &ld, step)
}

fn ld_library_path_spike(spike_lib: &Path, workload_build: &Path) -> String {
    let mut parts = vec![
        spike_lib.display().to_string(),
        workload_build.display().to_string(),
    ];
    if let Ok(prev) = env::var("LD_LIBRARY_PATH") {
        if !prev.is_empty() {
            parts.push(prev);
        }
    }
    parts.join(":")
}

pub fn worker_shm(name: String) -> Result<(), String> {
    let cs = CString::new(name).map_err(|_| "worker-shm: name contains NUL".to_string())?;
    if !cs.as_bytes().starts_with(b"/") {
        return Err("worker-shm: POSIX shm name must start with '/'".into());
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
        .map_err(|_| "spike shm: name contains NUL".to_string())?;

    let mut map = ShmMap::create_new(&name, shm::BEBOP_SHM_SIZE, false)
        .map_err(|e| format!("spike shm: create {e}"))?;

    let nm = name
        .to_str()
        .map_err(|_| "spike shm: name is not valid UTF-8".to_string())?;
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
    if log::log_enabled!(log::Level::Debug) {
        debug!(
            "spike cmd: {} {} {} {}",
            spike.display(),
            SPIKE_EXT,
            pk.display(),
            elf.display()
        );
    }

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
            return Err(format!("failed to spawn spike: {e}"));
        }
    };

    shm::rpc_shutdown(map.as_bebop());

    let wst = worker.wait().map_err(|e| format!("worker wait: {e}"))?;
    map.set_unlink_on_drop(true);
    if !wst.success() {
        return Err(format!("worker exited with {:?}", wst.code()));
    }
    if !st.success() {
        return Err(format!(
      "spike exited with {:?} (see spike/pk output above; pk often uses this as main's return code)",
      st.code()
    ));
    }
    Ok(())
}
