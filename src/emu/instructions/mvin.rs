use super::super::config::{BankConfig, BANK_NUM, BANK_SIZE};
/// MVIN 指令实现 (funct=24)
/// 功能：从内存加载数据到 bank
///
/// 宏定义：bb_mvin(mem_addr, bank_id, depth, stride)
/// xs1 = BB_BANK0(bank_id) | BB_WR | FIELD(mem_addr, 27, 58)
/// xs2 = FIELD(depth, 0, 9) | FIELD(stride, 10, 28)
use log::{error, info};

/// MVIN 指令执行
///
/// # Arguments
/// * `xs1` - 包含 bank_id 和 mem_addr
/// * `xs2` - 包含 depth 和 stride
/// * `memory` - 主内存
/// * `banks` - Bank 内存数组
/// * `bank_configs` - Bank 配置数组
pub fn execute_mvin(
    xs1: u64,
    xs2: u64,
    memory: &[u8],
    banks: &mut [Vec<u8>],
    bank_configs: &[BankConfig],
) -> u64 {
    // 解码 xs1：提取 bank_id 和 mem_addr
    let bank_id = xs1 & 0x1F; // bits 0-4
    let mem_addr = (xs1 >> 27) & 0xFFFFFFFF; // bits 27-58 (32 位地址)

    // 解码 xs2：提取 depth 和 stride
    let depth = xs2 & 0x3FF; // bits 0-9 (10 bits)
    let stride = (xs2 >> 10) & 0x7FFFF; // bits 10-28 (19 bits)

    info!(
        "MVIN: mem_addr=0x{:x}, bank_id={}, depth={}, stride={}",
        mem_addr, bank_id, depth, stride
    );

    if bank_id >= BANK_NUM as u64 {
        error!("MVIN: Invalid bank_id={}", bank_id);
        return 0;
    }

    if !bank_configs[bank_id as usize].allocated {
        error!("MVIN: Bank {} not allocated", bank_id);
        return 0;
    }

    // 从内存读取数据到 bank
    // 每次读取 16 字节（2 个 u64），stride 也表示 16 字节的倍数
    let actual_stride = if stride == 0 { 1 } else { stride };
    for i in 0..depth {
        // 每次读取 16 字节（2 个 u64）
        let addr = mem_addr + (i * 16 * actual_stride);
        // 读取第一个 u64
        let value_low = read_u64_from_memory(memory, addr);
        // 读取第二个 u64（偏移 8 字节）
        let value_high = read_u64_from_memory(memory, addr + 8);

        // 写入 bank（按 16 字节偏移存储，保持 2 个 u64）
        let bank_offset = (i * 16) as usize;
        if bank_offset + 16 <= BANK_SIZE {
            banks[bank_id as usize][bank_offset..bank_offset + 8]
                .copy_from_slice(&value_low.to_le_bytes());
            banks[bank_id as usize][bank_offset + 8..bank_offset + 16]
                .copy_from_slice(&value_high.to_le_bytes());
        }
    }

    0
}

/// 从内存读取 u64，地址按 memory.len() 取模以支持 guest VA 映射
fn read_u64_from_memory(memory: &[u8], addr: u64) -> u64 {
    let len = memory.len();
    let mut bytes = [0u8; 8];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = memory[((addr as usize) + i) % len];
    }
    u64::from_le_bytes(bytes)
}
