# BEMU 与 Spike 集成说明

本目录实现 **BEMU**（Bebop Emulator，Buckyball 自定义指令 Golden Model）与 **Spike**（RISC-V ISA 模拟器）的集成：guest 执行 **custom-0**（opcode `0x0b`）时，Spike 通过 RoCC 扩展 `bebop_rocc` 与 **独立 BEMU 进程** 通过 **POSIX 共享内存** 做 RPC，不再在 Spike 进程内 `dlopen` `libbemu.so`。

## 架构概览

- **BEMU**（`src/emu`）：Rust golden model；在 **`bebop worker-shm`** 子进程里跑（与 Spike 并行），直接调用 `Bemu::execute` / `write_memory` / `read_memory`。
- **共享内存布局**（须与 C++ 一致）：[`src/workload/bebop_shm.h`](../workload/bebop_shm.h) 与 [`src/shm/layout.rs`](../shm/layout.rs)。控制字段含 `req` / `ack`（C++ 侧用 `std::atomic_ref`），操作码：`HANDLE` / `SYNC` / `READ` / `SHUTDOWN`。
- **`bebop_rocc`**（[`src/workload/bebop_rocc.cc`](../workload/bebop_rocc.cc) → `libbebop_rocc.so`）：在 custom-0 路径上 `mmap` 环境变量 **`BEBOP_SHM_NAME`** 指向的段，通过 `req`/`ack` 与 worker 同步；**MVIN** 仍从 Spike MMU 读块，经 **`OP_SYNC`** 写入 BEMU；**MVOUT** 经 **`OP_READ`** 取回再写 MMU；普通指令走 **`OP_HANDLE`**。
- **`bebop spike-test`**：CLI 在 [`src/cli.rs`](../cli.rs)；仿真流程在 [`src/bebop.rs`](../bebop.rs)（创建 shm → `spawn` **`worker-shm`** → 设置 `BEBOP_SHM_NAME` 与 `LD_LIBRARY_PATH` 后启动 `spike --extension=bebop_rocc pk <elf>` → **`rpc_shutdown`** → `shm_unlink`）。

程序中的自定义指令为 RISC-V custom-0；funct7 / rs1 / rs2 对应 BEMU 的 funct、xs1、xs2。MVIN/MVOUT 使用 guest 虚地址；BEMU 内地址按 512KB 取模，与 Spike 同步后语义一致。

## 完整流程（按顺序执行）

### 方式 A：`bebop` CLI（推荐）

在仓库根目录（需 `spike`、`pk`、RISC-V 交叉编译器在 `PATH` 中，例如 `nix develop`）。先 **`cargo build --release`** 得到 `./target/release/bebop`。

```bash
cargo build --release
./target/release/bebop workload
./target/release/bebop spike-test
./target/release/bebop spike-test --all
```

- **`cargo build --release`**：bebop CLI、`libbemu.so` 等 Rust workspace。
- **`bebop workload`**：仅对 **`src/workload`** 执行 **`cmake` + `ninja`**（RISC-V ELF 与 `libbebop_rocc.so`）。
- **`bebop spike-test`**：不自动构建；缺产物时会报错并提示先执行 **`bebop workload`**。

### 方式 B：只构建 workload、不跑测试

```bash
cmake -S src/workload -B src/workload/build -G Ninja
ninja -C src/workload/build
```

（需要 bebop CLI 时在仓库根执行 **`cargo build --release`**。）

跑 Spike 测试请用 **`bebop spike-test`** / **`bebop spike-test --all`**。

## 测试内容

| 测试程序 | 覆盖指令 | 说明 |
|----------|----------|------|
| `test_bemu_custom` | MSET | 分配/释放 bank，检查返回值 |
| `test_bemu_mvin_mvout` | MSET, MVIN, MVOUT | 写 buffer → MVIN → bank → MVOUT → 另一 buffer，比对数据 |
| `test_bemu_matmul` | MSET, MVIN, MUL_WARP16, MVOUT | I×2I=2I 矩阵乘，校验结果 |
| `test_bemu_transpose` | MSET, MVIN, TRANSPOSE, MVOUT | 16×16 矩阵转置，校验 |
| `test_bemu_integration` | 全部 | MSET→MVIN(A,B)→MUL_WARP16→MVOUT(C)→TRANSPOSE→MVOUT(Ct)，校验 C=B、Ct=B^T |

## 配置文件

`BEMU` 运行时从 `src/emu/config.toml` 读取配置。

- 默认路径：`src/emu/config.toml`
- 可通过环境变量 `BEMU_CONFIG` 指定自定义路径
- 读取或解析失败会直接报错退出，不会静默回退默认行为

## 文件说明

| 路径 | 说明 |
|-----|------|
| `src/emu/` | BEMU 实现（Rust） |
| `src/emu/interface/capi_exports.rs` | C API（仍可供其他宿主 `dlopen`） |
| `src/shm/` | POSIX shm、`worker-shm`、与 `bebop_shm.h` 对齐的布局 |
| `src/workload/` | RISC-V 测试 C 程序、`bebop_rocc.cc`、`bebop_shm.h`、`bebop_insn.h`、`CMakeLists.txt` |
