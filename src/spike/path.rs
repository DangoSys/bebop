use std::path::PathBuf;

use crate::utils::path;

pub fn path_rocc_so() -> Result<PathBuf, String> {
    if let Ok(v) = std::env::var("BEBOP_ROCC_SO") {
        let p = PathBuf::from(v);
        let p = p
            .canonicalize()
            .map_err(|e| format!("BEBOP_ROCC_SO: {e}"))?;
        if !p.is_file() {
            return Err(format!("BEBOP_ROCC_SO is not a file: {}", p.display()));
        }
        return Ok(p);
    }
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
    Err(format!(
        "libbebop_rocc.so not found at {} (set BEBOP_ROCC_SO to absolute path)",
        p.display()
    ))
}

pub fn path_system_pk_bin() -> Result<PathBuf, String> {
    path::path_find_in_system_path("pk")
}

pub fn path_system_spike_bin() -> Result<PathBuf, String> {
    path::path_find_in_system_path("spike")
}
