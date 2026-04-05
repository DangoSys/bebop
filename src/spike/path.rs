use std::path::PathBuf;

use crate::utils::path;

pub fn path_rocc_so() -> Result<PathBuf, String> {
    let cur = path::path_current_bebop_bin()?;
    let cur_so = cur
        .parent()
        .ok_or("current bebop has no parent")?
        .join("../lib/libbebop_rocc.so");
    let p = cur_so
        .canonicalize()
        .map_err(|e| format!("canonicalize libbebop_rocc.so: {e}"))?;
    if p.is_file() {
        return Ok(p);
    }
    Err(format!("libbebop_rocc.so not found at {}", p.display()))
}

pub fn path_system_pk_bin() -> Result<PathBuf, String> {
    path::path_find_in_system_path("pk")
}

pub fn path_system_spike_bin() -> Result<PathBuf, String> {
    path::path_find_in_system_path("spike")
}
