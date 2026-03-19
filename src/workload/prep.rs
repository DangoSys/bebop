//! cmake + ninja: RISC-V test ELFs and `libbebop_rocc.so` (no Cargo).

use std::process::Command;

use log::info;

use super::{build_dir, dir, TEST_ELF_NAMES};

pub(super) fn run() -> Result<(), String> {
    let wl = dir();
    let out = build_dir();
    if !wl.is_dir() {
        return Err(format!("workload dir missing: {}", wl.display()));
    }
    info!("cmake: {} -> {}", wl.display(), out.display());
    let st = Command::new("cmake")
        .args([
            "-S",
            wl.to_str().ok_or("workload path is not UTF-8")?,
            "-B",
            out.to_str().ok_or("build path is not UTF-8")?,
            "-G",
            "Ninja",
        ])
        .status()
        .map_err(|e| format!("failed to run cmake: {e}"))?;
    if !st.success() {
        return Err("cmake failed".into());
    }
    let mut ninja = Command::new("ninja");
    ninja.arg("-C").arg(&out);
    for t in TEST_ELF_NAMES {
        ninja.arg(*t);
    }
    ninja.arg("bebop_rocc");
    info!("ninja: build targets in {}", out.display());
    let st = ninja
        .status()
        .map_err(|e| format!("failed to run ninja: {e}"))?;
    if !st.success() {
        return Err("ninja failed".into());
    }
    Ok(())
}
