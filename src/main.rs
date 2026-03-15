mod emu;

use clap::{Parser, Subcommand};
use emu::interface::{BemuSpikeInterface, SpikeCallbackParams, SpikeCallbacks};
use log::{debug, error, info};

#[derive(Parser)]
#[command(name = "bebop", about = "Bebop BEMU - Buckyball Emulator")]
struct Cli {
    /// 启用详细日志输出
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 运行 BEMU 测试
    Test {
        /// 要执行的测试指令类型
        #[arg(short, long, default_value = "all")]
        instruction: String,
    },
    /// 执行单个指令
    Execute {
        /// 功能码 (funct)
        #[arg(short, long)]
        funct: u32,
        /// 源操作数 1 (xs1)
        #[arg(long, default_value_t = 0)]
        xs1: u64,
        /// 源操作数 2 (xs2)
        #[arg(long, default_value_t = 0)]
        xs2: u64,
    },
    /// 显示 BEMU 信息
    Info,
}

fn main() {
    // 初始化日志
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    // 根据 verbose 标志设置日志级别
    if cli.verbose {
        log::set_max_level(log::LevelFilter::Debug);
    }

    match cli.command {
        Some(Commands::Test { instruction }) => {
            if let Err(e) = run_tests(&instruction, cli.verbose) {
                error!("测试失败：{:?}", e);
                std::process::exit(1);
            }
        }
        Some(Commands::Execute { funct, xs1, xs2 }) => {
            if let Err(e) = execute_instruction(funct, xs1, xs2, cli.verbose) {
                error!("指令执行失败：{:?}", e);
                std::process::exit(1);
            }
        }
        Some(Commands::Info) => {
            show_info();
        }
        None => {
            println!("Bebop BEMU - Buckyball Emulator");
            println!("使用 -h 或 --help 查看可用命令");
        }
    }
}

/// 运行 BEMU 测试
fn run_tests(instruction: &str, verbose: bool) -> Result<(), String> {
    info!("开始运行 BEMU 测试，指令类型：{}", instruction);

    let mut interface = BemuSpikeInterface::with_verbose(verbose);

    match instruction {
        "all" => {
            test_mset(&mut interface)?;
            test_mvin(&mut interface)?;
            test_mvout(&mut interface)?;
            test_matmul(&mut interface)?;
        }
        "mset" => {
            test_mset(&mut interface)?;
        }
        "mvin" => {
            test_mvin(&mut interface)?;
        }
        "mvout" => {
            test_mvout(&mut interface)?;
        }
        "matmul" => {
            test_matmul(&mut interface)?;
        }
        _ => {
            return Err(format!("未知的指令类型：{}", instruction));
        }
    }

    // 打印统计信息
    let stats = interface.get_stats();
    info!("测试完成，共执行 {} 条指令", stats.instructions_executed);

    Ok(())
}

/// 测试 MSET 指令（内存分配）
fn test_mset(interface: &mut BemuSpikeInterface) -> Result<(), String> {
    info!("测试 MSET 指令（内存分配）");

    // 测试 1：分配 bank 0: 4 行 x 4 列
    info!("  - 分配 bank 0 (4x4)");
    let params = SpikeCallbackParams::new(23, 0, 4 | (4 << 5) | (1 << 10));
    interface
        .handle_custom_instruction(&params)
        .map_err(|e| format!("MSET 分配失败：{:?}", e))?;

    // 验证：写入数据到 bank 0 并读取验证
    info!("  - 验证：写入数据到 bank 0 并读取");
    let test_pattern = vec![0xAAu8; 1024];  // 测试数据模式
    interface.sync_memory(0x100, &test_pattern)
        .map_err(|e| format!("同步测试数据失败：{:?}", e))?;
    
    // 使用 MVIN 将数据加载到 bank 0
    interface
        .handle_custom_instruction(&SpikeCallbackParams::new(24, 0 | (0x100 << 27), 8 | (1 << 10)))
        .map_err(|e| format!("MVIN 加载失败：{:?}", e))?;
    
    // 使用 MVOUT 从 bank 0 读出数据
    interface
        .handle_custom_instruction(&SpikeCallbackParams::new(25, 0 | (0x200 << 27), 8 | (1 << 10)))
        .map_err(|e| format!("MVOUT 读取失败：{:?}", e))?;
    
    // TODO: 需要添加从内存读取数据的接口来验证

    // 测试 2：释放 bank 0
    info!("  - 释放 bank 0");
    let params_release = SpikeCallbackParams::new(23, 0, 0);
    interface
        .handle_custom_instruction(&params_release)
        .map_err(|e| format!("MSET 释放失败：{:?}", e))?;

    info!("MSET 测试通过");
    Ok(())
}

/// 测试 MVIN 指令（内存加载）
fn test_mvin(interface: &mut BemuSpikeInterface) -> Result<(), String> {
    info!("测试 MVIN 指令（内存加载）");

    // 步骤 1：分配 bank 0 (16x1)，可以存储 16 行数据
    info!("  - 分配 bank 0 (16x1)");
    interface
        .handle_custom_instruction(&SpikeCallbackParams::new(23, 0, 16 | (1 << 5) | (1 << 10)))
        .map_err(|e| format!("分配 bank 失败：{:?}", e))?;

    // 步骤 2：准备测试数据
    // MVIN 每次读取 16 字节（2 个 u64），stride=1 表示每次跳过 16 字节
    // 8 行数据，每行 16 字节，总共需要 8 * 16 = 128 字节
    let mut test_data: Vec<u8> = vec![0; 128];
    
    // 填充测试数据：每个 16 字节块包含 2 个 u64
    for i in 0..8 {
        let mem_offset = i * 16;  // 内存中每行间隔 16 字节
        let value1 = (i + 1) as u64;  // 第一个 u64：递增的值
        let value2 = (i + 1) as u64 * 10;  // 第二个 u64：10 倍的递增值
        // 写入 16 字节到内存（2 个 u64）
        test_data[mem_offset..mem_offset + 8].copy_from_slice(&value1.to_le_bytes());
        test_data[mem_offset + 8..mem_offset + 16].copy_from_slice(&value2.to_le_bytes());
    }

    // 步骤 3：同步测试数据到内存地址 0x100
    info!("  - 同步测试数据到内存 0x100");
    interface
        .sync_memory(0x100, &test_data)
        .map_err(|e| format!("内存同步失败：{:?}", e))?;

    // 步骤 4：执行 MVIN：从地址 0x100 加载 8 行数据到 bank 0
    info!("  - 执行 MVIN 加载数据 (depth=8, stride=1)");
    let xs1 = 0 | (0x100 << 27); // bank_id=0, mem_addr=0x100
    let xs2 = 8 | (1 << 10); // depth=8, stride=1 (每行间隔 16 字节)
    let params = SpikeCallbackParams::new(24, xs1, xs2);
    interface
        .handle_custom_instruction(&params)
        .map_err(|e| format!("MVIN 执行失败：{:?}", e))?;

    // 步骤 5：使用 MVOUT 将数据读回内存地址 0x200
    info!("  - 执行 MVOUT 读回数据到 0x200 (depth=8, stride=1)");
    interface
        .handle_custom_instruction(&SpikeCallbackParams::new(25, 0 | (0x200 << 27), 8 | (1 << 10)))
        .map_err(|e| format!("MVOUT 执行失败：{:?}", e))?;

    // 步骤 6：从内存读取数据并验证
    info!("  - 验证：读取内存 0x200 并比较数据");
    // MVOUT 写入时也是每次 16 字节，间隔 16 字节，所以需要读取全部 128 字节
    let read_back = interface
        .read_memory(0x200, 128)
        .map_err(|e| format!("读取内存失败：{:?}", e))?;

    // 验证数据：检查每个 16 字节块中的 2 个 u64
    let mut errors = Vec::new();
    for i in 0..8 {
        let mem_offset = i * 16;
        let expected_value1 = (i + 1) as u64;
        let expected_value2 = (i + 1) as u64 * 10;
        
        // 读取第一个 u64
        let actual_value1 = u64::from_le_bytes(
            read_back[mem_offset..mem_offset + 8].try_into().unwrap()
        );
        // 读取第二个 u64
        let actual_value2 = u64::from_le_bytes(
            read_back[mem_offset + 8..mem_offset + 16].try_into().unwrap()
        );
        
        if actual_value1 != expected_value1 {
            errors.push(format!(
                "  位置 0x{:03x}: 第一个 u64 预期 {}, 实际 {}",
                0x200 + mem_offset, expected_value1, actual_value1
            ));
        }
        if actual_value2 != expected_value2 {
            errors.push(format!(
                "  位置 0x{:03x}: 第二个 u64 预期 {}, 实际 {}",
                0x200 + mem_offset + 8, expected_value2, actual_value2
            ));
        }
    }

    if !errors.is_empty() {
        let mut error_msg = String::from("MVIN/MVOUT 数据验证失败:\n");
        for err in &errors {
            error_msg.push_str(err);
            error_msg.push('\n');
        }
        return Err(error_msg);
    }

    info!("  - 验证通过：读取数据与预期完全一致 (8 行 x 16 字节)");
    info!("MVIN 测试通过");
    Ok(())
}

/// 测试 MVOUT 指令（内存存储）
fn test_mvout(interface: &mut BemuSpikeInterface) -> Result<(), String> {
    info!("测试 MVOUT 指令（内存存储）");

    // 步骤 1：分配 bank 1 (16x1)，可以存储 16 行数据
    info!("  - 分配 bank 1 (16x1)");
    interface
        .handle_custom_instruction(&SpikeCallbackParams::new(23, 1, 16 | (1 << 5) | (1 << 10)))
        .map_err(|e| format!("分配 bank 失败：{:?}", e))?;

    // 步骤 2：准备测试数据并同步到内存
    // MVIN 每次读取 16 字节（2 个 u64），stride=1 表示每次跳过 16 字节
    let mut test_data: Vec<u8> = vec![0; 128];
    
    // 填充测试数据：每个 16 字节块包含 2 个 u64
    for i in 0..8 {
        let mem_offset = i * 16;  // 内存中每行间隔 16 字节
        let value1 = (i * 2 + 1) as u64;  // 第一个 u64：奇数序列
        let value2 = (i * 2 + 1) as u64 * 10;  // 第二个 u64：10 倍的奇数序列
        // 写入 16 字节到内存（2 个 u64）
        test_data[mem_offset..mem_offset + 8].copy_from_slice(&value1.to_le_bytes());
        test_data[mem_offset + 8..mem_offset + 16].copy_from_slice(&value2.to_le_bytes());
    }

    info!("  - 同步数据到内存 0x200");
    interface
        .sync_memory(0x200, &test_data)
        .map_err(|e| format!("内存同步失败：{:?}", e))?;

    // 步骤 3：执行 MVIN 将数据加载到 bank 1
    info!("  - 执行 MVIN 加载数据到 bank 1 (depth=8, stride=1)");
    let xs1_in = 1 | (0x200 << 27);
    let xs2 = 8 | (1 << 10);
    interface
        .handle_custom_instruction(&SpikeCallbackParams::new(24, xs1_in, xs2))
        .map_err(|e| format!("MVIN 执行失败：{:?}", e))?;

    // 步骤 4：执行 MVOUT：将 bank 1 的数据存储到地址 0x300
    info!("  - 执行 MVOUT 存储数据到内存 0x300 (depth=8, stride=1)");
    let xs1_out = 1 | (0x300 << 27); // bank_id=1, mem_addr=0x300
    let params = SpikeCallbackParams::new(25, xs1_out, xs2);
    interface
        .handle_custom_instruction(&params)
        .map_err(|e| format!("MVOUT 执行失败：{:?}", e))?;

    // 步骤 5：从内存读取数据并验证
    info!("  - 验证：读取内存 0x300 并比较数据");
    // MVOUT 写入时也是每次 16 字节，间隔 16 字节，所以需要读取全部 128 字节
    let read_back = interface
        .read_memory(0x300, 128)
        .map_err(|e| format!("读取内存失败：{:?}", e))?;

    // 验证数据：检查每个 16 字节块中的 2 个 u64
    let mut errors = Vec::new();
    for i in 0..8 {
        let mem_offset = i * 16;
        let expected_value1 = (i * 2 + 1) as u64;  // 奇数序列
        let expected_value2 = (i * 2 + 1) as u64 * 10;  // 10 倍的奇数序列
        
        // 读取第一个 u64
        let actual_value1 = u64::from_le_bytes(
            read_back[mem_offset..mem_offset + 8].try_into().unwrap()
        );
        // 读取第二个 u64
        let actual_value2 = u64::from_le_bytes(
            read_back[mem_offset + 8..mem_offset + 16].try_into().unwrap()
        );
        
        if actual_value1 != expected_value1 {
            errors.push(format!(
                "  位置 0x{:03x}: 第一个 u64 预期 {}, 实际 {}",
                0x300 + mem_offset, expected_value1, actual_value1
            ));
        }
        if actual_value2 != expected_value2 {
            errors.push(format!(
                "  位置 0x{:03x}: 第二个 u64 预期 {}, 实际 {}",
                0x300 + mem_offset + 8, expected_value2, actual_value2
            ));
        }
    }

    if !errors.is_empty() {
        let mut error_msg = String::from("MVOUT 数据验证失败:\n");
        for err in &errors {
            error_msg.push_str(err);
            error_msg.push('\n');
        }
        return Err(error_msg);
    }

    info!("  - 验证通过：读取数据与预期完全一致 (8 行 x 16 字节)");
    info!("MVOUT 测试通过");
    Ok(())
}

/// 测试矩阵乘法指令
    fn test_matmul(interface: &mut BemuSpikeInterface) -> Result<(), String> {
        info!("测试矩阵乘法指令");
    
        // 步骤 1：分配三个 bank（每个 16x16）
        // 注意：先释放可能已分配的 bank（避免重复分配错误）
        info!("  - 释放 bank 0, 1, 2（如果已分配）");
        let _ = interface.handle_custom_instruction(&SpikeCallbackParams::new(23, 0, 0));
        let _ = interface.handle_custom_instruction(&SpikeCallbackParams::new(23, 1, 0));
        let _ = interface.handle_custom_instruction(&SpikeCallbackParams::new(23, 2, 0));
        
        info!("  - 分配 bank 0, 1, 2 (16x16)");
        interface
            .handle_custom_instruction(&SpikeCallbackParams::new(23, 0, 16 | (16 << 5) | (1 << 10)))
            .map_err(|e| format!("分配 bank 0 失败：{:?}", e))?;
        interface
            .handle_custom_instruction(&SpikeCallbackParams::new(23, 1, 16 | (16 << 5) | (1 << 10)))
            .map_err(|e| format!("分配 bank 1 失败：{:?}", e))?;
        interface
            .handle_custom_instruction(&SpikeCallbackParams::new(23, 2, 16 | (16 << 5) | (1 << 10)))
            .map_err(|e| format!("分配 bank 2 失败：{:?}", e))?;

    // 步骤 2：初始化矩阵数据
    // 矩阵 A (bank 0)：单位矩阵 I
    // 矩阵 B (bank 1)：2I 矩阵（对角线为 2，其余为 0）
    // 预期结果 C (bank 2)：I × 2I = 2I
    // 注意：MVIN 每次读取 16 字节（2 个 u64），所以矩阵元素在内存中按 16 字节间隔存储
    // 每个 16 字节块包含 2 个 u64 元素
    info!("  - 初始化矩阵 A (bank 0) 为单位矩阵");
    let mut matrix_a = vec![0u8; 256 * 8];  // 256 个元素，每个元素 8 字节，连续存储
    for i in 0..256 {
        let offset = i * 8;  // 每个元素间隔 8 字节（连续存储）
        // 单位矩阵：对角线元素为 1
        let row = i / 16;
        let col = i % 16;
        let value = if row == col { 1u64 } else { 0 };
        matrix_a[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    interface.sync_memory(0x1000, &matrix_a).map_err(|e| format!("同步矩阵 A 失败：{:?}", e))?;
    
    // 使用 MVIN 将矩阵 A 加载到 bank 0
    // 矩阵 A：16x16，每个元素 8 字节，总共 256 个元素
    // MVIN 每次读取 16 字节（2 个 u64），所以需要 depth=128（128 * 16 = 2048 字节 = 256 * 8 字节）
    // stride=1（每次读取间隔 16 字节）
    let xs1 = 0 | (0x1000 << 27);
    let xs2 = 128 | (1 << 10);  // depth=128, stride=1
    interface
        .handle_custom_instruction(&SpikeCallbackParams::new(24, xs1, xs2))
        .map_err(|e| format!("加载矩阵 A 失败：{:?}", e))?;

    info!("  - 初始化矩阵 B (bank 1) 为 2I 矩阵");
    let mut matrix_b = vec![0u8; 256 * 8];  // 256 个元素，每个元素 8 字节，连续存储
    for i in 0..256 {
        let offset = i * 8;
        let row = i / 16;
        let col = i % 16;
        let value = if row == col { 2u64 } else { 0 };
        matrix_b[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    interface.sync_memory(0x2000, &matrix_b).map_err(|e| format!("同步矩阵 B 失败：{:?}", e))?;
    
    // 使用 MVIN 将矩阵 B 加载到 bank 1
    let xs1 = 1 | (0x2000 << 27);
    let xs2 = 128 | (1 << 10);  // depth=128, stride=1
    interface
        .handle_custom_instruction(&SpikeCallbackParams::new(24, xs1, xs2))
        .map_err(|e| format!("加载矩阵 B 失败：{:?}", e))?;

    // 步骤 3：执行矩阵乘法：bank0 * bank1 -> bank2
    info!("  - 执行 MUL_WARP16: I × 2I -> bank2");
    let xs1 = 0 | (1 << 5) | (2 << 10); // op1=0, op2=1, wr=2
    let xs2 = 16; // iter=16
    let params = SpikeCallbackParams::new(32, xs1, xs2);
    interface
        .handle_custom_instruction(&params)
        .map_err(|e| format!("矩阵乘法执行失败：{:?}", e))?;

    // 步骤 4：使用 MVOUT 将结果从 bank 2 读出到内存
    info!("  - 读取结果矩阵 C (bank 2)");
    // MVOUT depth=128, stride=1（每次写入 16 字节，包含 2 个 u64 元素）
    interface
        .handle_custom_instruction(&SpikeCallbackParams::new(25, 2 | (0x3000 << 27), 128 | (1 << 10)))
        .map_err(|e| format!("读取结果矩阵失败：{:?}", e))?;

    // 步骤 5：从内存读取结果并验证
    info!("  - 验证：检查结果矩阵是否为 2I");
    // 结果矩阵在内存中连续存储，需要读取 256 * 8 字节
    let result_data = interface
        .read_memory(0x3000, 256 * 8)
        .map_err(|e| format!("读取结果矩阵失败：{:?}", e))?;

    // 验证结果矩阵：对角线应该是 2，其余为 0
    let mut errors = Vec::new();
    for i in 0..256 {
        let offset = i * 8;  // 每个元素间隔 8 字节（连续存储）
        let value = u64::from_le_bytes(
            result_data[offset..offset + 8].try_into().unwrap()
        );
        let row = i / 16;
        let col = i % 16;
        let expected = if row == col { 2 } else { 0 };
        if value != expected {
            errors.push(format!(
                "  位置 [{}][{}]: 预期 {}, 实际 {}",
                row, col, expected, value
            ));
        }
    }

    if !errors.is_empty() {
        let mut error_msg = String::from("矩阵乘法结果验证失败:\n");
        for err in &errors {
            error_msg.push_str(err);
            error_msg.push('\n');
        }
        return Err(error_msg);
    }

    info!("  - 验证通过：结果矩阵 C = 2I (对角线为 2，其余为 0)");
    info!("矩阵乘法测试通过");
    Ok(())
}

/// 执行单个指令
fn execute_instruction(funct: u32, xs1: u64, xs2: u64, verbose: bool) -> Result<(), String> {
    info!("执行指令：funct={}, xs1=0x{:x}, xs2=0x{:x}", funct, xs1, xs2);

    let mut interface = BemuSpikeInterface::with_verbose(verbose);
    let params = SpikeCallbackParams::new(funct, xs1, xs2);

    match interface.handle_custom_instruction(&params) {
        Ok(result) => {
            info!("指令执行成功，结果：0x{:x}", result);
            println!("指令执行成功，结果：0x{:x}", result);
            Ok(())
        }
        Err(e) => {
            error!("指令执行失败：{:?}", e);
            Err(format!("指令执行失败：{:?}", e))
        }
    }
}

/// 显示 BEMU 信息
fn show_info() {
    println!("Bebop BEMU - Buckyball Emulator");
    println!("================================");
    println!("版本：0.1.0");
    println!();
    println!("支持的指令类型:");
    println!("  - MSET (funct=23): 内存分配/释放");
    println!("  - MVIN (funct=24): 内存到 Bank 的数据加载");
    println!("  - MVOUT (funct=25): Bank 到内存的数据存储");
    println!("  - MUL_WARP16 (funct=32): 16 位矩阵乘法");
    println!("  - TRANSPOSE (funct=34): 矩阵转置");
    println!();
    println!("内存配置:");
    println!("  - Bank 数量：32");
    println!("  - 每个 Bank 大小：16KB");
    println!("  - 总内存大小：512KB");
    println!();
    println!("使用示例:");
    println!("  bebop test --instruction all      # 运行所有测试");
    println!("  bebop execute --funct 23 --xs1 0 --xs2 260  # 执行 MSET 指令");
    println!("  bebop info                        # 显示 BEMU 信息");
    println!("  bebop --verbose test              # 启用详细日志运行测试");
}

