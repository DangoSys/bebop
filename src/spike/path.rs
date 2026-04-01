use std::path::PathBuf;

use crate::utils::path;

pub fn path_rocc_so() -> Result<PathBuf, String> {
    let sys = path::path_system_bebop_bin()?;
    let sys_so = sys
        .parent()
        .ok_or("system bebop has no parent")?
        .join("../lib/libbebop_rocc.so");
    if let Ok(p) = sys_so.canonicalize() {
        if p.is_file() {
            return Ok(p);
        }
    }

    let cur = path::path_current_bebop_bin()?;
    let cur_so = cur
        .parent()
        .ok_or("current bebop has no parent")?
        .join("../lib/libbebop_rocc.so");
    if let Ok(p) = cur_so.canonicalize() {
        if p.is_file() {
            return Ok(p);
        }
    }

    let p = sys_so
        .canonicalize()
        .map_err(|e| format!("canonicalize system rocc so: {e}"))?;
    if p.is_file() {
        return Ok(p);
    }
    Err(format!("libbebop_rocc.so not found; tried {}", p.display()))
}

pub fn path_system_pk_bin() -> Result<PathBuf, String> {
    path::path_find_in_system_path("pk")
}

pub fn path_system_spike_bin() -> Result<PathBuf, String> {
    path::path_find_in_system_path("spike")
}
