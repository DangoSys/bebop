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
    let bank_id = xs1 & 0xFF; // bits 0-7 (BB_BANK0)
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

    // 每行字节数由 MSET 的 cols 决定：cols * 16B
    let cols = bank_configs[bank_id as usize].cols;
    let line_blocks = if cols == 0 { 1 } else { cols as usize };
    let line_bytes = line_blocks * 16;

    // 从内存读取数据到 bank。
    // stride 以 16B block 为单位，按 line_blocks 扩展到整行跨度。
    let actual_stride = if stride == 0 { 1 } else { stride };
    for i in 0..depth {
        let addr = mem_addr + (i * 16 * actual_stride * line_blocks as u64);
        let bank_offset = (i as usize) * line_bytes;
        if bank_offset + line_bytes <= BANK_SIZE {
            for j in 0..line_bytes {
                banks[bank_id as usize][bank_offset + j] =
                    read_u8_from_memory(memory, addr + j as u64);
            }
        } else {
            error!(
                "MVIN: bank_offset out of range: bank_id={}, bank_offset={}, line_bytes={}, depth={}, stride={}",
                bank_id, bank_offset, line_bytes, depth, stride
            );
            return 1;
        }
    }

    0
}

/// 从内存读取 u8，地址按 memory.len() 取模以支持 guest VA 映射
fn read_u8_from_memory(memory: &[u8], addr: u64) -> u8 {
    let len = memory.len();
    memory[(addr as usize) % len]
}
