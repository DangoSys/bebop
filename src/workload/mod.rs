//! Spike workloads: RISC-V tests, `bebop_rocc`, headers, CMake (`bebop workload`).
//! Rust 侧在仓库根 **`cargo build --release`**。Spike 仿真在 `src/bebop.rs`。

use std::path::PathBuf;

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
];

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `src/workload`（源码 + `CMakeLists.txt`）。
pub fn dir() -> PathBuf {
    repo_root().join("src/workload")
}

pub fn build_dir() -> PathBuf {
    dir().join("build")
}
