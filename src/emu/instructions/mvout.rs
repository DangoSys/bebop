use super::super::config::{BankConfig, BANK_NUM, BANK_SIZE};
/// MVOUT 指令实现 (funct=25)
/// 功能：从 bank 存储数据到内存
///
/// 宏定义：bb_mvout(mem_addr, bank_id, depth, stride)
/// xs1 = BB_BANK0(bank_id) | BB_RD0 | FIELD(mem_addr, 27, 58)
/// xs2 = FIELD(depth, 0, 9) | FIELD(stride, 10, 28)
use log::{error, info};

/// MVOUT 指令执行
///
/// # Arguments
/// * `xs1` - 包含 bank_id 和 mem_addr
/// * `xs2` - 包含 depth 和 stride
/// * `memory` - 主内存
/// * `banks` - Bank 内存数组
/// * `bank_configs` - Bank 配置数组
pub fn execute_mvout(
    xs1: u64,
    xs2: u64,
    memory: &mut [u8],
    banks: &[Vec<u8>],
    bank_configs: &[BankConfig],
) -> u64 {
    // 解码 xs1：提取 bank_id 和 mem_addr
    let bank_id = xs1 & 0xFF; // bits 0-7 (BB_BANK0)
    let mem_addr = (xs1 >> 27) & 0xFFFFFFFF; // bits 27-58 (32 位地址)

    // 解码 xs2：提取 depth 和 stride
    let depth = xs2 & 0x3FF; // bits 0-9
    let stride = (xs2 >> 10) & 0x7FFFF; // bits 10-28

    info!(
        "MVOUT: mem_addr=0x{:x}, bank_id={}, depth={}, stride={}",
        mem_addr, bank_id, depth, stride
    );

    if bank_id >= BANK_NUM as u64 {
        error!("MVOUT: Invalid bank_id={}", bank_id);
        return 0;
    }

    if !bank_configs[bank_id as usize].allocated {
        error!("MVOUT: Bank {} not allocated", bank_id);
        return 0;
    }

    // 每行字节数由 MSET 的 cols 决定：cols * 16B
    let cols = bank_configs[bank_id as usize].cols;
    let line_blocks = if cols == 0 { 1 } else { cols as usize };
    let line_bytes = line_blocks * 16;

    // 从 bank 读取数据写入内存。
    // stride 以 16B block 为单位，按 line_blocks 扩展到整行跨度。
    let actual_stride = if stride == 0 { 1 } else { stride };
    for i in 0..depth {
        let bank_offset = (i as usize) * line_bytes;
        if bank_offset + line_bytes > BANK_SIZE {
            error!(
                "MVOUT: bank_offset out of range: bank_id={}, bank_offset={}, line_bytes={}, depth={}, stride={}",
                bank_id, bank_offset, line_bytes, depth, stride
            );
            return 1;
        }
        let addr = mem_addr + (i * 16 * actual_stride * line_blocks as u64);
        for j in 0..line_bytes {
            write_u8_to_memory(
                memory,
                addr + j as u64,
                banks[bank_id as usize][bank_offset + j],
            );
        }
    }

    0
}

/// 写入 u8 到内存，地址按 memory.len() 取模
fn write_u8_to_memory(memory: &mut [u8], addr: u64, value: u8) {
    let len = memory.len();
    memory[(addr as usize) % len] = value;
}
