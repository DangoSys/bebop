# BEMU 与 Spike 集成说明

本目录实现 **BEMU**（Bebop Emulator，Buckyball 自定义指令 Golden Model）与 **Spike**（RISC-V ISA 模拟器）的集成：guest 执行 **custom-0**（opcode `0x0b`）时，Spike 通过 RoCC 扩展 `bebop_rocc` 与 **独立 BEMU 进程** 通过 **POSIX 共享内存** 做 RPC，不再在 Spike 进程内 `dlopen` `libbemu.so`。

## 架构概览

- **BEMU**（`src/emu`）：Rust golden model；在 **`bebop worker-shm`** 子进程里跑（与 Spike 并行），直接调用 `Bemu::execute` / `write_memory` / `read_memory`。
- **共享内存布局**（须与 C++ 一致）：[`src/spike/bebop_shm.h`](../spike/bebop_shm.h) 与 [`src/shm/layout.rs`](../shm/layout.rs)。**`BEBOP_SHM_SIZE` = 8192**。每路 lane 含 `req` / `ack` + `bebop_msg_t`（含 **`bank_digest`** 供 difftest）。Cosim 布局：**`cmd_bemu` / `cmd_rtl` / `mem_bemu` / `mem_rtl`** 四 lane；**`bebop bemu`** 仅使用 **`cmd_bemu` + `mem_bemu`**。
- **`bebop_rocc`**（[`src/spike/bebop_rocc.cc`](../spike/bebop_rocc.cc) → `libbebop_rocc.so`）：在 custom-0 路径上 `mmap` 环境变量 **`BEBOP_SHM_NAME`** 指向的段，通过 `req`/`ack` 与 worker 同步；**MVIN** 仍从 Spike MMU 读块，经 **`OP_SYNC`** 写入 BEMU；**MVOUT** 经 **`OP_READ`** 取回再写 MMU；普通指令走 **`OP_HANDLE`**。
- **`bebop bemu`**：仅 Spike + **`bemu-tests`**（只做 BEMU golden）。
- **Node 协议**（[`src/node/node.rs`](../node/node.rs)）：`bebop` 主进程为 node0；`runner` 为 Spike 与侧车分配 **`--node-file`** 中的递增 **`node_id`**。**`bemu-tests`** 与 **`verilator-engine`**（Unix cosim）各自 `alloc_node_id`。
- **`bebop verilator`**：Spike + **`verilator-engine` 仅**；内部按 **`--extlib=<libbebop_rocc.so绝对路径>` + `--extension=bebop_rocc`** 启动 Spike，只用 **`cmd_rtl` + `mem_rtl`**（无 `bemu-tests`，无 BEMU 侧 `b0=` step 行）。
- **`bebop difftest`**：Spike + **`bemu-tests` + `verilator-engine`**；两路 cmd/mem 并行，**`rd` 必须一致**；**`BEBOP_DIFFTEST=1`** 时 Spike 再校验两路 **`bank_digest`**（与 BEMU `cosim_aggregate_banks_digest` 同 FNV 规则）。`bebop_cosim_banks` 仅在 **`issue_start`** 时解码 `funct`，**`banks_busy`**（如 `mul64` 多拍）会并入 `rtl_busy`，避免采样过早。**`bebop bemu`** 仅走 **`cmd_bemu` + `mem_bemu`**。**`--step`**：BEMU 的 **`b0=…`** 与 FNV digest 仍非同一指标。加 **`--all-banks`** 时打印全部 bank。Cosim 需 **Unix**。

程序中的自定义指令为 RISC-V custom-0；funct7 / rs1 / rs2 对应 BEMU 的 funct、xs1、xs2。MVIN/MVOUT 使用 guest 虚地址；BEMU 内地址按 512KB 取模，与 Spike 同步后语义一致。



已提供 **双 cmd + 双 mem** 

## 完整流程（按顺序执行）

```bash
nix develop
cargo build --release
./target/release/bebop bemu /path/to/your-test-linux
./target/release/bebop verilator /path/to/your-test-linux
./target/release/bebop difftest /path/to/your-test-linux

./target/release/bebop bemu /home/daiyongyuan/buckyball/bb-tests/output/workloads/src/CTest/toy/ctest_vecunit_tiled_matmul-linux --step
./target/release/bebop verilator /home/daiyongyuan/buckyball/bb-tests/output/workloads/src/CTest/toy/ctest_vecunit_tiled_matmul-linux --step
./target/release/bebop difftest /home/daiyongyuan/buckyball/bb-tests/output/workloads/src/CTest/toy/ctest_vecunit_tiled_matmul-linux --step

./target/release/bebop bemu /home/daiyongyuan/buckyball/bb-tests/output/workloads/src/CTest/toy/ctest_vecunit_matmul_random1-linux --step
./target/release/bebop verilator /home/daiyongyuan/buckyball/bb-tests/output/workloads/src/CTest/toy/ctest_vecunit_matmul_random1-linux --step
./target/release/bebop difftest /home/daiyongyuan/buckyball/bb-tests/output/workloads/src/CTest/toy/ctest_vecunit_matmul_random1-linux --step
```

- **`cargo build --release`**：bebop CLI、`libbemu.so` 等。`build.rs` 会自动给 Verilator 的 `make` 设置并行度（优先 `BEBOP_MAKE_JOBS`，其次 `NIX_BUILD_CORES`，默认 `16`），并默认保留 `vl_bebop` 目录做增量构建。
- 需要强制清理并全量重编 Verilator 产物时，使用 `BEBOP_CLEAN_VL=1 cargo build --release`。
- **`cmake` / `ninja`**：在 **`src/spike`** 生成 **`src/spike/build/libbebop_rocc.so`**（CMake 需能 `find_program(spike)`）。
- **`bebop bemu <ELF>`** / **`bebop verilator <ELF>`**：传入已构建好的 RISC-V Linux 测例可执行文件的完整路径；缺 **`libbebop_rocc.so`** 会直接报错退出。运行时不依赖额外配置 `BEBOP_ROCC_SO`。


## 配置文件

`BEMU` 配置来源（**不读环境变量**）：

- **显式**：任意子命令可用的全局参数 **`bebop --config /path/to/config.toml …`**（会传给 `bemu-tests` 子进程）。  
- **默认**（未传 `--config` 时按顺序）：  
  1. 与 `bebop` 同 prefix：**`../share/bebop/config.toml`**（如 Nix `bebop-with-rocc`）  
  2. 本机源码树仍存在时：**`src/emu/configs/config.toml`**（相对编译时的 crate 根）  

以上皆不可用或解析失败时直接报错退出。

## 文件说明

| 路径 | 说明 |
|-----|------|
| `src/emu/` | BEMU（Rust）、[`runner.rs`](runner.rs)（`bemu-tests` RPC）、[`vl_engine.rs`](vl_engine.rs)（`verilator-engine`，Unix） |
| `src/emu/interface/capi_exports.rs` | C API（仍可供其他宿主 `dlopen`） |
| `src/shm/` | POSIX shm、与 `bebop_shm.h` 对齐的布局 |
| `src/spike/` | `bebop_rocc.cc`、`bebop_shm.h`、`CMakeLists.txt`、`runner.rs` |
