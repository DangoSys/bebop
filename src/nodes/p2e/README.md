# P2E 构建流程总结（基于实际构建结果）

## 完整构建流程

基于 `p2e_control_path` 的实际构建结果：

```
[1] vvac -bc -f flist.lst -top dut_top
    ↓
    生成 vvacDir/
    ├── vvac_by_mod/filelist      # 包装后的 RTL 文件列表
    ├── runtime.dbdir/            # 运行时数据库
    └── runtimeDir/               # 运行时环境（包含 libvCtb.so）
        ├── include/              # ICtb.h, expFun.h, stub.h
        ├── lib/lib_arm/          # libvCtb.so + 依赖库
        └── rtcfg/                # 运行时配置

[2] vsyn -F vvacDir/vvac_by_mod/filelist -top xepic_vvac_top -o top.vm
    ↓
    生成网表 top.vm

[3] vcom vcom_compile.tcl
    ↓
    生成 fpgaCompDir/ (FPGA 工程)

[4] make -C fpgaCompDir all
    ↓
    生成 bitstream.bit

[5] vdbg debug.tcl
    ↓
    烧录并运行
```

## 我们的 Rust 实现策略

### 阶段 1：构建比特流（bebop-p2e build）

```rust
BitstreamBuilder::build() {
    // Step 1: vvac - 生成 DPI-C wrapper + libvCtb.so
    vvac -bc -f flist.lst -top top_module
    
    // Step 2: vsyn - 综合
    vsyn -F vvacDir/vvac_by_mod/filelist -top xepic_vvac_top -o top.vm
    
    // Step 3: vcom - 系统编译
    vcom vcom_compile.tcl
    
    // Step 4: PNR - 布局布线
    make -C fpgaCompDir all
}
```

### 阶段 2：运行仿真（bebop-p2e run）

```rust
// 1. vdbg 加载比特流和初始化内存
VdbgSession::new()
    .connect()
    .download_bitstream()
    .memory_write(0x80000000, "kernel.bin")
    .init_hardware()

// 2. DPI-C 实时交互
P2ESimulator::new("P0", "./out", "./out/vvacDir/runtimeDir/rtcfg")
    .reset()
    .run_until_exit()
```

