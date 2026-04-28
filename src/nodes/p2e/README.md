# bebop-p2e

P2E (Prototyping to Emulation) Rust 包，用于芯华章 FPGA 仿真。

## 功能

- **DDR 预加载** - 通过 backdoor 将 kernel image 写入 DDR (0x80000000)
- **SCU 控制** - 通过 DPI-C 与 RTL 的 System Control Unit 交互
- **纯 Rust** - 直接 FFI 调用 DPI-C，无 C 中间层
- **比特流生成** - 通过 feature flag 控制

## 使用

### 基本示例

```rust
use bebop_p2e::{P2ESimulator, ScuController};

fn main() -> Result<(), String> {
    let sim = P2ESimulator::new()?;
    
    // DDR 预加载
    let kernel = std::fs::read("kernel.bin")?;
    sim.load_image(0x80000000, &kernel)?;
    
    // 复位
    sim.reset()?;
    
    // UART 输出
    ScuController::uart_puts("Hello!\n")?;
    
    // 运行仿真
    let exit_code = sim.run_until_exit()?;
    println!("Exit code: {}", exit_code);
    
    Ok(())
}
```

### 编译

```bash
# 基本编译
cargo build --features p2e

# 包含比特流生成
export BEBOP_VERILOG_DIR=/path/to/verilog
cargo build --features p2e,build-bitstream
```

### 环境变量

- `VVAC_LIB_DIR` - 芯华章 VVAC 库路径
- `BEBOP_VERILOG_DIR` - Verilog 文件目录（默认：`/home/wanghui/Code/buckyball/arch/build`）

## 地址映射

- `0x80000000` - DDR 基地址
- `0x60000000` - 仿真退出地址
- `0x60020000` - UART TX 地址

## DPI-C 接口

### RTL 导出（Rust 调用）

```systemverilog
export "DPI-C" task waitNCycles;
```

### Rust 导出（RTL 调用）

```systemverilog
import "DPI-C" context function void p2e_init();
import "DPI-C" context function void p2e_ddr_backdoor_write(input longint addr, input byte data[], input int len);
import "DPI-C" context function int scu_mmio_write(input int addr, input int data);
import "DPI-C" context function int scu_mmio_read(input int addr);
```
