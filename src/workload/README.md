# Workload (Spike)

RISC-V bare-metal tests, RoCC extension `bebop_rocc`, and shared-memory headers. CMake output: `build/` under this directory.

- **`prep.rs`** / **`workload::cmake_ninja()`** — only **`cmake` + `ninja`**（CLI **`bebop workload`**）。
- Rust workspace：仓库根 **`cargo build --release`**。
- Spike 仿真：**`bebop spike-test`**（`src/bebop.rs`）。

典型顺序：`cargo build --release` → `bebop workload` → `bebop spike-test`。
