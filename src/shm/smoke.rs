//! CLI smoke test for `shm::posix`.

use std::ffi::CString;
use std::sync::atomic::{AtomicU64, Ordering};

use super::posix::PosixShm;

static SHM_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn run(size: usize) -> Result<(), String> {
    if size < 4 {
        return Err("shm-smoke: size must be >= 4".into());
    }
    let seq = SHM_SEQ.fetch_add(1, Ordering::Relaxed);
    let name = CString::new(format!("/bebop_{}_{}", std::process::id(), seq))
        .map_err(|_| "shm-smoke: name contains NUL".to_string())?;
    let mut shm = PosixShm::create_exclusive(&name, size).map_err(|e| format!("shm-smoke: {e}"))?;
    let slice = shm.as_mut_slice();
    let pat = 0xdeadbeef_u32.to_le_bytes();
    slice[0..4].copy_from_slice(&pat);
    if slice[0..4] != pat {
        return Err("shm-smoke: readback mismatch".into());
    }
    Ok(())
}
