# BEMU

Bemu is an instruction emulator targeting NPUs.

## src 目录结构

`src` 目录是项目的主要源代码目录，包含以下核心组件：

### 1. main.rs - 程序主入口

**功能职责**：
- 提供命令行接口（CLI），支持用户与 BEMU 模拟器交互
- 实现三个主要命令：
  - `test`：运行 BEMU 指令测试套件
  - `execute`：执行单个自定义指令
  - `info`：显示 BEMU 模拟器信息
- 集成日志系统，支持详细日志输出模式
- 实现错误处理和退出机制

**命令行选项**：
- `-v, --verbose`：启用详细日志输出
- `test --instruction <type>`：运行指定类型的指令测试
- `execute --funct <n> --xs1 <n> --xs2 <n>`：执行单个指令

**使用示例**：
```bash
# 显示帮助信息
bebop --help

# 运行所有指令测试
bebop test --instruction all

# 执行 MSET 指令
bebop execute --funct 23 --xs1 0 --xs2 260

# 启用详细日志
bebop --verbose test
```

### 2. emu/ - BEMU 模拟器核心模块

BEMU（Bebop Emulator）是项目的核心模拟器模块，提供 NPU 自定义指令的软件实现。

**模块组成**：
- `mod.rs`：模块入口文件
- `bemu.rs`：BEMU 模拟器主实现
- `config.rs`：配置和常量定义
- `instructions/`：指令实现子模块
- `interface/`：接口层子模块

### 3. tauri/ - GUI 前端模块

基于 Tauri 框架的图形用户界面模块（可选功能）。

**目录结构**：
- `src-tauri/`：Tauri 后端 Rust 代码
- `src/`：React 前端 JavaScript 代码
- `index.html`：GUI 入口 HTML 文件

### 4. wasm/ - WebAssembly 模块

将 BEMU 模拟器编译为 WebAssembly，支持在浏览器中运行。

**目录结构**：
- `src/lib.rs`：WASM 导出接口
- `web/`：Web 页面资源

---

## emu 目录详细结构

`emu` 目录是 BEMU 模拟器的核心实现，采用模块化设计，包含以下子模块：

### 目录结构

```
emu/
├── mod.rs                 # 模块入口
├── README.md              # BEMU 模块说明
├── bemu.rs                # BEMU 模拟器主实现
├── config.rs              # 配置和常量定义
├── instructions/          # 指令实现模块
│   ├── mod.rs            # 指令模块入口
│   ├── mset.rs           # MSET 指令实现
│   ├── mvin.rs           # MVIN 指令实现
│   ├── mvout.rs          # MVOUT 指令实现
│   └── matmul.rs         # 矩阵乘法指令实现
└── interface/            # 接口层模块
    ├── mod.rs            # 接口层入口
    ├── decoder.rs        # 指令解码器
    ├── memory_map.rs     # 内存映射器
    └── spike_interface.rs # Spike 回调接口
```

### 核心文件说明

#### 1. mod.rs - 模块入口

**功能职责**：
- 声明并导出所有子模块
- 提供模块级别的公共 API

**导出内容**：
- `Bemu`：BEMU 模拟器主结构体
- `instructions`：指令实现模块
- `interface`：接口层模块

#### 2. bemu.rs - BEMU 模拟器主实现

**功能职责**：
- 实现 BEMU 模拟器的核心状态管理
- 提供指令执行引擎
- 管理内存和 Bank 资源
- 收集和执行统计信息

**核心结构**：
- `Bemu`：模拟器主结构体
  - `memory`：主内存空间（512KB）
  - `banks`：32 个 Bank，每个 16KB
  - `bank_configs`：Bank 配置信息
  - `stats`：执行统计信息

**主要方法**：
- `new()`：创建新的 BEMU 实例
- `set_verbose()`：设置详细日志模式
- `execute()`：执行自定义指令
- `write_memory()`：写入内存
- `read_memory()`：读取内存
- `get_stats()`：获取统计信息
- `reset_stats()`：重置统计信息

**支持的指令**：
- MSET（funct=23）：内存分配/释放
- MVIN（funct=24）：内存到 Bank 的数据加载
- MVOUT（funct=25）：Bank 到内存的数据存储
- MUL_WARP16（funct=32）：16 位矩阵乘法
- TRANSPOSE（funct=34）：矩阵转置

#### 3. config.rs - 配置和常量定义

**功能职责**：
- 定义内存配置常量
- 定义数据结构

**核心常量**：
- `BANK_NUM`：Bank 数量（32）
- `BANK_SIZE`：每个 Bank 的大小（16KB = 16384 字节）
- `TOTAL_MEMORY_SIZE`：总内存大小（512KB）

**数据结构**：
- `BemuStats`：执行统计信息
  - `instructions_executed`：已执行指令数
  - `matmul_count`：矩阵乘法次数
- `BankConfig`：Bank 配置
  - `allocated`：是否已分配
  - `row`：行数
  - `col`：列数

#### 4. instructions/ - 指令实现模块

包含所有 Buckyball 自定义指令的具体实现。

##### 4.1 mod.rs - 指令模块入口

**功能职责**：
- 声明并导出所有指令实现子模块

##### 4.2 mset.rs - MSET 指令实现

**指令功能**：内存分配/释放

**指令编码**：
- `funct`：23
- `xs1[4:0]`：bank_id（Bank 编号）
- `xs2[0]`：alloc（1=分配，0=释放）
- `xs2[9:5]`：row（行数）
- `xs2[14:10]`：col（列数）

**实现内容**：
- `execute_mset()`：MSET 指令执行函数
- 单元测试：验证内存分配和释放功能

##### 4.3 mvin.rs - MVIN 指令实现

**指令功能**：从主内存加载数据到 Bank

**指令编码**：
- `funct`：24
- `xs1[4:0]`：bank_id（Bank 编号）
- `xs1[31:27]`：mem_addr[31:12]（内存地址高 5 位）
- `xs2[9:0]`：depth（数据深度/行数）
- `xs2[28:10]`：stride（跨步，以 16 字节为单位）

**实现内容**：
- `execute_mvin()`：MVIN 指令执行函数
- 从内存读取数据并写入 Bank
- 支持跨步访问模式
- 单元测试：验证数据加载功能

##### 4.4 mvout.rs - MVOUT 指令实现

**指令功能**：将 Bank 数据存储到主内存

**指令编码**：
- `funct`：25
- `xs1[4:0]`：bank_id（Bank 编号）
- `xs1[31:27]`：mem_addr[31:12]（内存地址高 5 位）
- `xs2[9:0]`：depth（数据深度/行数）
- `xs2[28:10]`：stride（跨步，以 16 字节为单位）

**实现内容**：
- `execute_mvout()`：MVOUT 指令执行函数
- 从 Bank 读取数据并写入内存
- 支持跨步访问模式
- 单元测试：验证数据存储功能

##### 4.5 matmul.rs - 矩阵乘法指令实现

**指令功能**：16 位矩阵乘法和矩阵转置

**MUL_WARP16 指令编码**：
- `funct`：32
- `xs1[4:0]`：op1（操作数 1 的 Bank 编号）
- `xs1[9:5]`：op2（操作数 2 的 Bank 编号）
- `xs1[14:10]`：wr（写回结果的 Bank 编号）
- `xs2[9:0]`：iter（迭代次数/矩阵维度）

**TRANSPOSE 指令编码**：
- `funct`：34
- `xs1[4:0]`：bank_id（Bank 编号）
- `xs2[9:0]`：size（矩阵大小）

**实现内容**：
- `execute_mul_warp16()`：矩阵乘法执行函数
- `execute_transpose()`：矩阵转置执行函数
- 单元测试：验证矩阵运算功能

#### 5. interface/ - 接口层模块

提供 BEMU 与外部仿真器（如 Spike）的接口，包含三个核心子模块。

##### 5.1 mod.rs - 接口层入口

**功能职责**：
- 声明并导出所有接口层子模块
- 提供公共 API 导出

**导出内容**：
- `InstructionDecoder`：指令解码器
- `MemoryMapper`：内存映射器
- `BemuSpikeInterface`：Spike 接口实现
- `SpikeCallbacks`：回调函数 trait
- `SpikeCallbackParams`：回调参数
- `SpikeError`：错误类型
- `MemoryRegion`：内存区域
- `MemoryPermission`：内存权限

##### 5.2 decoder.rs - 指令解码器

**功能职责**：
- 解析和解码 Buckyball 自定义指令
- 识别指令类型
- 提取指令参数
- 提供指令验证功能

**核心结构**：
- `InstructionDecoder`：指令解码器
  - `decode()`：解码指令
  - `decode_from_instr_word()`：从指令字解码
  - `is_valid()`：验证指令有效性
  - `instruction_details()`：生成指令详细信息
- `DecodedInstruction`：解码结果
- `InstructionType`：指令类型枚举

**支持的指令类型**：
- MSET
- MVIN
- MVOUT
- MUL_WARP16
- TRANSPOSE

##### 5.3 memory_map.rs - 内存映射器

**功能职责**：
- 管理物理地址到虚拟地址的转换
- 管理内存区域（注册、映射、取消映射）
- 实现访问权限检查
- 提供地址转换缓存优化

**核心结构**：
- `MemoryMapper`：内存映射器
  - `register_region()`：注册内存区域
  - `map_region()`：映射内存区域
  - `unmap_region()`：取消映射
  - `phys_to_virt()`：物理地址转虚拟地址
  - `virt_to_phys()`：虚拟地址转物理地址
  - `is_phys_accessible()`：检查物理地址可访问性
  - `is_virt_accessible()`：检查虚拟地址可访问性
  - `clear_cache()`：清除地址转换缓存
- `MemoryRegion`：内存区域描述
- `MemoryPermission`：内存权限枚举
  - `ReadOnly`：只读
  - `WriteOnly`：只写
  - `ReadWrite`：读写
  - `None`：无权限

**常量**：
- `PAGE_SIZE`：内存页大小（4KB）

##### 5.4 spike_interface.rs - Spike 回调接口

**功能职责**：
- 提供与 Spike 模拟器集成的标准化接口
- 定义回调函数 trait
- 实现错误处理机制
- 提供 C 兼容的 FFI 接口

**核心结构**：
- `BemuSpikeInterface`：BEMU Spike 接口实现
  - `new()`：创建新实例
  - `with_verbose()`：创建带详细日志的实例
  - `handle_custom_instruction()`：处理自定义指令
  - `sync_memory()`：同步内存数据
  - `get_stats()`：获取统计信息
  - `reset_stats()`：重置统计信息
- `SpikeCallbacks`：回调函数 trait
  - `handle_custom_instruction()`：处理自定义指令
  - `sync_memory()`：同步内存
  - `get_stats()`：获取统计
  - `reset_stats()`：重置统计
  - `get_version()`：获取版本信息
- `SpikeCallbackParams`：回调参数
  - `funct`：功能码
  - `xs1`：源操作数 1
  - `xs2`：源操作数 2
  - `pc`：程序计数器（可选）
  - `timestamp`：时间戳（可选）
- `SpikeError`：错误类型枚举
  - `UnknownInstruction`：未知指令
  - `InvalidMemoryAccess`：无效内存访问
  - `BankNotAllocated`：Bank 未分配
  - `InvalidParameter`：参数错误
  - `InternalError`：内部错误

**C 兼容接口**：
- `spike_bemu_create_interface()`：创建接口实例
- `spike_bemu_free_interface()`：释放接口实例
- `spike_bemu_handle_custom()`：处理自定义指令
- `spike_bemu_sync_memory()`：同步内存

---

## 技术规格

### 内存配置

- **Bank 数量**：32
- **每个 Bank 大小**：16KB（16384 字节）
- **总内存大小**：512KB（524288 字节）
- **内存页大小**：4KB（4096 字节）

### 支持的指令集

| 指令名称   | funct | 功能描述           |
|-----------|-------|------------------|
| MSET      | 23    | 内存分配/释放     |
| MVIN      | 24    | 内存到 Bank 加载  |
| MVOUT     | 25    | Bank 到内存存储   |
| MUL_WARP16| 32    | 16 位矩阵乘法     |
| TRANSPOSE | 34    | 矩阵转置          |

### 依赖项

- Rust 2021 Edition
- clap 4.x：命令行参数解析
- log 0.4：日志系统
- env_logger 0.11：日志输出

### 测试覆盖

项目包含 26 个单元测试，覆盖：
- 指令解码功能（5 个测试）
- 内存映射功能（5 个测试）
- Spike 接口功能（6 个测试）
- BEMU 核心指令功能（10 个测试）

---

## 快速开始

### 编译项目

```bash
cd /home/daiyongyuan/bebop
cargo build
```

### 运行测试

```bash
cargo test
```

### 使用命令行

```bash
# 查看帮助
cargo run -- --help

# 显示 BEMU 信息
cargo run -- info

# 运行所有测试
cargo run -- test --instruction all

# 执行单个指令
cargo run -- execute --funct 23 --xs1 0 --xs2 260

# 启用详细日志
cargo run -- --verbose test
```

---

## 模块间关系

```
main.rs (命令行入口)
    │
    └── emu (BEMU 模拟器模块)
            │
            ├── bemu.rs (模拟器核心)
            │       ├── 内存管理
            │       ├── 指令执行引擎
            │       └── 统计信息
            │
            ├── instructions/ (指令实现)
            │       ├── mset.rs
            │       ├── mvin.rs
            │       ├── mvout.rs
            │       └── matmul.rs
            │
            └── interface/ (接口层)
                    ├── decoder.rs (指令解码)
                    ├── memory_map.rs (地址映射)
                    └── spike_interface.rs (Spike 集成)
```

---

## 设计原则

1. **模块化**：各功能模块边界清晰，接口明确
2. **独立性**：emu 模块作为独立 Rust 项目，无外部依赖
3. **可扩展性**：接口层预留标准化接口，便于后续集成
4. **可测试性**：完善的单元测试覆盖
5. **可维护性**：详细的日志输出和错误处理机制

---


