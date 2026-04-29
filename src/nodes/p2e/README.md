# bebop-p2e

P2E (Prototyping to Emulation) Rust 包，用于芯华章 FPGA 仿真。

## 功能

- **比特流构建** - vsyn → vcom → PNR 完整流程
- **DDR 预加载** - 通过 vdbg TCL backdoor 将 kernel/rootfs 写入 DDR
- **DPI-C 实时交互** - 通过 libvCtb.so 与 FPGA 上的 SCU 通信
- **UART 输出** - 实时 UART 日志收集
- **纯 Rust** - 无 C++ 中间层，直接 FFI 调用

## 架构

```
┌─────────────────────────────────────────────────────────────┐
│                    P2E Simulation Flow                       │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Phase 1: Build Bitstream (vsyn → vcom → PNR)               │
│  Phase 2: Load Bitstream & Init Memory (vdbg TCL)           │
│  Phase 3: Run Simulation (Rust + DPI-C)                     │
│  Phase 4: Cleanup                                            │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## 使用

### 环境

```bash
export HPEC_HOME=/path/to/hpec
```

P2E 构建输入通过 `ARCH_CONFIG` 指定，和 Verilator node 一样从 buckyball 根目录的 `arch/build/<config>` 取 Verilog：

```bash
ARCH_CONFIG=sims.p2e.P2EToyConfig cargo build -p bebop-p2e
```

`libvCtb.so` 不需要通过环境变量指定。Rust build script 会运行 `vvac`，并从相对输出目录自动查找：

```text
./out/vvacDir/runtimeDir/lib/lib_arm/libvCtb.so
```

如果 `ARCH_CONFIG` 缺失、`vvac` 不在 `PATH`，或 `vvac` 没有生成 `libvCtb.so`，构建会直接失败。

### 基本示例

```rust
use bebop_p2e::{BitstreamBuilder, P2ESimulator};

fn main() -> Result<(), String> {
    // 1. 构建比特流。原始 Verilog 和 VVAC runtime 已由 build.rs 准备好。
    let builder = BitstreamBuilder::new()
        .vvac_top_module("xepic_vvac_top")
        .output_dir("./out")
        .hw_config("hw-config.hdf");
    builder.build()?;

    // 2. 运行仿真
    let mut sim = P2ESimulator::new("P0", "./out", "./out/vvacDir/runtimeDir/rtcfg")?;
    sim.reset()?;

    let exit_code = sim.run_until_exit()?;
    println!("Exit code: {}", exit_code);

    Ok(())
}
```

### 运行示例

```bash
# 编译 P2E 包，同时由 build.rs 运行 vvac 并链接 libvCtb.so
ARCH_CONFIG=sims.p2e.P2EToyConfig cargo build -p bebop-p2e

# 构建 bitstream
bebop p2e --buildbitstream

# 运行 workload
bebop p2e --runworkload
```

## API 文档

### BitstreamBuilder

比特流构建器，封装 vsyn → vcom → PNR 流程。

```rust
let builder = BitstreamBuilder::new()
    .vvac_top_module("xepic_vvac_top")
    .output_dir("./out")
    .hw_config("hw-config.hdf");

builder.build()?;
```

### VdbgSession

vdbg 会话管理器，负责比特流加载和 DDR backdoor。

```rust
let mut vdbg = VdbgSession::new()
    .bitstream("bitstream.bit")
    .hw_config("hw-config.hdf")
    .fpga_id("0.A")
    .work_dir("./out");

vdbg.connect()?;
vdbg.download_bitstream()?;
vdbg.memory_write(0x80000000, "kernel.bin")?;
vdbg.init_hardware()?;
```

### P2ESimulator

P2E 仿真器，通过 DPI-C 与 FPGA 实时交互。

```rust
let mut sim = P2ESimulator::new(
    "P0",                              // FPGA ID
    "./out",                           // case_home
    "./out/vvacDir/runtimeDir/rtcfg"  // rtcfg_path
)?;

// 复位
sim.reset()?;

// 推进时钟
sim.step(1000)?;

// 运行直到退出
let exit_code = sim.run_until_exit()?;

// 运行指定时间
sim.run_for(300)?;  // 300 秒

// 获取 UART 日志
let log = sim.get_uart_log();
```

## 地址映射

- `0x80000000` - DDR 基地址（kernel）
- `0x82000000` - DDR rootfs 地址
- `0x60000000` - 仿真退出地址
- `0x60020000` - UART TX 地址

## DPI-C 接口

### RTL 导出（Rust 调用）

```systemverilog
export "DPI-C" task waitNCycles;
```

```rust
unsafe { ffi::waitNCycles(100); }
```

### Rust 导出（RTL 调用）

```systemverilog
import "DPI-C" context function void p2e_init();
import "DPI-C" context function int scu_mmio_write(input int addr, input int data);
import "DPI-C" context function int scu_mmio_read(input int addr);
```

这些函数在 `ffi.rs` 中实现。

## 参考

- 芯华章 VVAC 文档
- `/home/wanghui/Code/buckyball/bebop/fpga-demo/examples/p2e_control_path`
- `/home/wanghui/Code/buckyball/bebop/fpga-demo/examples/p2e_ddr4_backdoor`
