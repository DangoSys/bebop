use std::path::PathBuf;

use crate::utils::path;

pub const SPIKE_EXT: &str = "--extension=bebop_rocc";

pub fn path_rocc_so() -> Result<PathBuf, String> {
    let bebop = path::path_system_bebop_bin()?;
    let p = bebop
        .parent()
        .ok_or("system bebop has no parent")?
        .join("../lib/libbebop_rocc.so");
    p.canonicalize()
        .map_err(|e| format!("canonicalize system rocc so: {e}"))
}

pub fn path_system_pk_bin() -> Result<PathBuf, String> {
    path::path_find_in_system_path("pk")
}

pub fn path_system_spike_bin() -> Result<PathBuf, String> {
    path::path_find_in_system_path("spike")
}
