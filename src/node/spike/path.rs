use std::path::PathBuf;

use crate::framework::utils::path;

pub fn path_rocc_so() -> Result<PathBuf, String> {
    let cur = path::path_current_bebop_bin()?;
    let exe_dir = cur.parent().ok_or("current bebop has no parent")?;
    let candidates = [
        exe_dir.join("../../src/spike/build/libbebop_rocc.so"),
        exe_dir.join("../lib/libbebop_rocc.so"),
    ];
    let mut tried = Vec::new();
    for p in &candidates {
        tried.push(p.display().to_string());
        if let Ok(c) = p.canonicalize() {
            if c.is_file() {
                return Ok(c);
            }
        }
    }
    Err(format!(
        "libbebop_rocc.so not found (tried: {}). For dev: cmake --build src/spike/build --target bebop_rocc. For install/Nix: place libbebop_rocc.so in ../lib relative to this binary.",
        tried.join(", ")
    ))
}

pub fn path_system_pk_bin() -> Result<PathBuf, String> {
    path::path_find_in_system_path("pk")
}

pub fn path_system_spike_bin() -> Result<PathBuf, String> {
    path::path_find_in_system_path("spike")
}
