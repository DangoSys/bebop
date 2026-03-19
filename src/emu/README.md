# BEMU 与 Spike 集成说明

本目录实现了 **BEMU**（Bebop Emulator，Buckyball 自定义指令 Golden Model）与 **Spike**（RISC-V ISA 模拟器）的集成：在 Spike 上运行真实 RISC-V 程序时，当程序执行到 **custom-0**（opcode 0x0b）自定义指令，Spike 会通过 RoCC 扩展自动调用 BEMU 执行指令语义。

## 架构概览

- **BEMU**（`src/emu`）：Rust 实现的 Buckyball 自定义指令 Golden Model，通过 C API（`bemu_create_interface`、`bemu_handle_custom` 等）对外提供接口。
- **libbemu.so**：由 `cargo build --release` 生成的动态库，导出上述 C API。
- **bebop_rocc**：Spike 的 customext 扩展（`examples/bebop_rocc.cc`，由 CMake/Ninja 构建为 `libbebop_rocc.so`），在 custom-0 指令命中时通过 **dlopen** 加载 libbemu.so 并调用 `bemu_handle_custom(funct, xs1, xs2)`，将返回值写回 rd。并对 **MVIN/MVOUT** 做内存同步：
  - **MVIN**（funct=24）：执行前按 xs1/xs2 解码的 mem_addr、depth、stride，从 Spike 的 MMU 读取对应 16 字节块，经 `bemu_sync_memory` 写入 BEMU 内存，再执行 BEMU 的 MVIN。
  - **MVOUT**（funct=25）：执行 BEMU 的 MVOUT 后，用 `bemu_read_memory` 从 BEMU 读出对应范围，再经 Spike MMU 写回 Spike 内存。

程序中的自定义指令编码为 RISC-V custom-0（opcode 0x0b），funct7 与 rs1/rs2 传递 BEMU 的 funct、xs1、xs2。这样程序用 load/store 访问的地址与 BEMU 的 MVIN/MVOUT 所用地址一致，Spike 与 BEMU 共享同一份“主存”视图。

## 完整流程（按顺序执行）

### 步骤 1：构建 libbemu.so

在仓库根目录：

```bash
nix develop
cargo build --release
```

得到 `target/release/libbemu.so`。


### 步骤 2：编译 RISC-V 测试程序

```bash
cd examples
cmake -S . -B build -G Ninja
ninja -C build test_bemu_custom
```

### 步骤 3：运行测试

```bash
cd examples
ninja -C build run
```

一键跑全部测试（MSET、MVIN/MVOUT、MUL_WARP16、TRANSPOSE、综合）：

```bash
cd examples
ninja -C build run-all
```

## 测试内容

| 测试程序 | 覆盖指令 | 说明 |
|----------|----------|------|
| `test_bemu_custom` | MSET | 分配/释放 bank，检查返回值 |
| `test_bemu_mvin_mvout` | MSET, MVIN, MVOUT | 写 buffer → MVIN → bank → MVOUT → 另一 buffer，比对数据 |
| `test_bemu_matmul` | MSET, MVIN, MUL_WARP16, MVOUT | I×2I=2I 矩阵乘，校验结果 |
| `test_bemu_transpose` | MSET, MVIN, TRANSPOSE, MVOUT | 16×16 矩阵转置，校验 |
| `test_bemu_integration` | 全部 | MSET→MVIN(A,B)→MUL_WARP16→MVOUT(C)→TRANSPOSE→MVOUT(Ct)，校验 C=B、Ct=B^T |

程序中的 MVIN/MVOUT 使用 guest 虚地址（如全局 buffer）；BEMU 内将地址按 512KB 取模，与 Spike 内存同步后语义一致。

## 配置文件

`BEMU` 运行时从 `src/emu/config.toml` 读取配置（TOML 序列化文件）。

- 默认路径：`src/emu/config.toml`
- 可通过环境变量 `BEMU_CONFIG` 指定自定义路径
- 读取或解析失败会直接报错退出，不会静默回退默认行为
- 当前实现使用定长数组，`bank_num / bank_width / bank_lines / matrix_size` 需与硬件常量一致

## 文件说明

| 路径 | 说明 |
|-----|------|
| `src/emu/` | BEMU 实现（Rust） |
| `src/emu/interface/capi_exports.rs` | BEMU C API 导出 |
| `examples/bebop_rocc.cc` | Spike customext 动态库实现（`libbebop_rocc.so`） |
| `examples/bebop_insn.h` | 测试程序用 custom-0 编码与封装 |
| `examples/test_bemu_custom.c` | 调用 MSET 的 C 测试 |
| `examples/CMakeLists.txt` | 编译测试程序与运行目标（`run` / `run-all`） |
