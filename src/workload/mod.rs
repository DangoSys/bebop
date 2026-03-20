//! Spike workloads: RISC-V tests, `bebop_rocc`, headers, CMake (`bebop workload`).
//! Rust 侧在仓库根 **`cargo build --release`**。Spike 仿真在 `src/bebop.rs`。
//!
//! `nix build` / `cargo install` 二进制里 **`CARGO_MANIFEST_DIR`** 指向沙箱源码路径，在运行时往往不存在。
//! 解析顺序：**`BEBOP_DIR`**（仓库根，须指向含 `src/workload/CMakeLists.txt` 的目录；`nix develop` shellHook 会设）→
//! 否则回退编译期 **`CARGO_MANIFEST_DIR`**（本机 **`cargo build`** / **`cargo run`** 时有效；`nix build` 产物需设 `BEBOP_DIR`）。

use std::env;
use std::path::{Path, PathBuf};

mod prep;

/// `cmake` + `ninja` → `src/workload/build`（ELFs + `libbebop_rocc.so`）。
pub fn cmake_ninja() -> Result<(), String> {
    prep::run()
}

/// Basenames of Spike test ELFs under [`build_dir`]（与 CMake `TESTS` 一致）。
pub const TEST_ELF_NAMES: &[&str] = &[
    "test_bemu_custom",
    "test_bemu_mvin_mvout",
    "test_bemu_matmul",
    "test_bemu_transpose",
    "test_bemu_integration",
    "test_vecunit_tiled_matmul",
];

fn is_bebop_repo_root(p: &Path) -> bool {
    p.join("src/workload/CMakeLists.txt").is_file()
}

/// 仓库根（含 `src/workload` 的那一层）。
pub fn repo_root() -> PathBuf {
    if let Ok(s) = env::var("BEBOP_DIR") {
        let r = PathBuf::from(s.trim());
        if is_bebop_repo_root(&r) {
            return r;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `src/workload`（源码 + `CMakeLists.txt`）。
pub fn dir() -> PathBuf {
    repo_root().join("src/workload")
}

pub fn build_dir() -> PathBuf {
    dir().join("build")
}
