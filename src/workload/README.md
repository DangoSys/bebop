# Workload (Spike)

RISC-V bare-metal tests, RoCC extension `bebop_rocc`, and shared-memory headers. CMake output: `build/` under this directory.

- **`prep.rs`** / **`workload::cmake_ninja()`** — only **`cmake` + `ninja`**（CLI **`bebop workload`**）。
- Rust workspace：仓库根 **`cargo build --release`**。
- Spike 仿真：**`bebop spike-test`**（`src/bebop.rs`）。

典型顺序：`cargo build --release` → `bebop workload` → `bebop spike-test`。

nix 安装的 `bebop`：必须设置 **`BEBOP_DIR`** 为克隆根目录；本机 **`cargo build`** 未设时则用编译期 crate 路径。若路径像 `/nix/store/.../build/...-source`，需带 **`BEBOP_DIR`** 或在本仓库重编译后再用。
