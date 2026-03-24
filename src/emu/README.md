# BEMU 与 Spike 集成说明

本目录实现 **BEMU**（Bebop Emulator，Buckyball 自定义指令 Golden Model）与 **Spike**（RISC-V ISA 模拟器）的集成：guest 执行 **custom-0**（opcode `0x0b`）时，Spike 通过 RoCC 扩展 `bebop_rocc` 与 **独立 BEMU 进程** 通过 **POSIX 共享内存** 做 RPC，不再在 Spike 进程内 `dlopen` `libbemu.so`。

## 架构概览

- **BEMU**（`src/emu`）：Rust golden model；在 **`bebop worker-shm`** 子进程里跑（与 Spike 并行），直接调用 `Bemu::execute` / `write_memory` / `read_memory`。
- **共享内存布局**（须与 C++ 一致）：[`src/spike/bebop_shm.h`](../spike/bebop_shm.h) 与 [`src/shm/layout.rs`](../shm/layout.rs)。控制字段含 `req` / `ack`（C++ 侧用 `std::atomic_ref`），操作码：`HANDLE` / `SYNC` / `READ` / `SHUTDOWN`。
- **`bebop_rocc`**（[`src/spike/bebop_rocc.cc`](../spike/bebop_rocc.cc) → `libbebop_rocc.so`）：在 custom-0 路径上 `mmap` 环境变量 **`BEBOP_SHM_NAME`** 指向的段，通过 `req`/`ack` 与 worker 同步；**MVIN** 仍从 Spike MMU 读块，经 **`OP_SYNC`** 写入 BEMU；**MVOUT** 经 **`OP_READ`** 取回再写 MMU；普通指令走 **`OP_HANDLE`**。
- **`bebop spike-test`**：CLI 在 [`src/cli.rs`](../cli.rs)；仿真流程在 [`src/spike/spike_runner.rs`](../spike/spike_runner.rs)（创建 shm → `spawn` **`worker-shm`** → 设置 `BEBOP_SHM_NAME` 与 `LD_LIBRARY_PATH` 后启动 `spike --extension=bebop_rocc pk <elf>` → **`rpc_shutdown`** → `shm_unlink`）。**`--step`**：每条 **RoCC 自定义指令**（`OP_HANDLE`）执行后，worker 打印 **`lat=<N>`**（`exec_latency::cycles_after_issue` → 各 `fXX_*.rs` 的 **`latency`**，启发式 issue→complete 周期），并为 **已 MSET 分配** 的 bank 各打印一个 **64-bit** 哈希（`b0=<16hex> …`，`std` 默认 `Hasher`）。**`BEBOP_STEP_BANKS=all`** 时打印全部 bank。非 RoCC 的 RISC‑V 指令不在此粒度内。

程序中的自定义指令为 RISC-V custom-0；funct7 / rs1 / rs2 对应 BEMU 的 funct、xs1、xs2。MVIN/MVOUT 使用 guest 虚地址；BEMU 内地址按 512KB 取模，与 Spike 同步后语义一致。

## 完整流程（按顺序执行）

```bash
cargo build --release
cmake -S src/spike -B src/spike/build -G Ninja
ninja -C src/spike/build bebop_rocc
./target/release/bebop spike-test /path/to/your-test-linux
```

- **`cargo build --release`**：bebop CLI、`libbemu.so` 等。
- **`cmake` / `ninja`**：在 **`src/spike`** 生成 **`src/spike/build/libbebop_rocc.so`**（CMake 需能 `find_program(spike)`）。
- **`bebop spike-test <ELF>`**：传入已构建好的 RISC-V Linux 测例可执行文件的完整路径；缺 **`libbebop_rocc.so`** 会直接报错退出。


## 配置文件

`BEMU` 运行时从 **`BEBOP_DIR`** 下的 `src/emu/configs/config.toml` 读取配置。

- 默认：设置 **`BEBOP_DIR`** 为 bebop 仓库根后，路径为 `src/emu/configs/config.toml`
- 可通过环境变量 `BEMU_CONFIG` 指定自定义路径
- 读取或解析失败会直接报错退出，不会静默回退默认行为

## 文件说明

| 路径 | 说明 |
|-----|------|
| `src/emu/` | BEMU 实现（Rust）、`worker.rs`（`worker-shm` RPC 侧车） |
| `src/emu/interface/capi_exports.rs` | C API（仍可供其他宿主 `dlopen`） |
| `src/shm/` | POSIX shm、与 `bebop_shm.h` 对齐的布局 |
| `src/spike/` | `bebop_rocc.cc`、`bebop_shm.h`、`CMakeLists.txt`、`spike_runner.rs` |
