/// MVOUT 指令实现 (funct=25)
/// 功能：从 bank 存储数据到内存
/// 
/// 宏定义：bb_mvout(mem_addr, bank_id, depth, stride)
/// xs1 = BB_BANK0(bank_id) | BB_RD0 | FIELD(mem_addr, 27, 58)
/// xs2 = FIELD(depth, 0, 9) | FIELD(stride, 10, 28)

use log::{error, info};
use super::super::config::{BANK_NUM, BANK_SIZE, BankConfig};

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
    let bank_id = xs1 & 0x1F;              // bits 0-4
    let mem_addr = (xs1 >> 27) & 0xFFFFFFFF; // bits 27-58 (32 位地址)
    
    // 解码 xs2：提取 depth 和 stride
    let depth = xs2 & 0x3FF;              // bits 0-9
    let stride = (xs2 >> 10) & 0x7FFFF;   // bits 10-28
    
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
    
    // 从 bank 读取数据写入内存
    // 每次读取 16 字节（2 个 u64），stride 也表示 16 字节的倍数
    let actual_stride = if stride == 0 { 1 } else { stride };
    for i in 0..depth {
        // 从 bank 读取 16 字节（2 个 u64）
        let bank_offset = (i * 16) as usize;
        if bank_offset + 16 > BANK_SIZE {
            break;
        }
        // 读取第一个 u64
        let value_low = u64::from_le_bytes(
            banks[bank_id as usize][bank_offset..bank_offset + 8]
                .try_into().unwrap()
        );
        // 读取第二个 u64
        let value_high = u64::from_le_bytes(
            banks[bank_id as usize][bank_offset + 8..bank_offset + 16]
                .try_into().unwrap()
        );
        
        // 写入内存（每次写入 16 字节，地址间隔为 16 字节 * stride）
        let addr = mem_addr + (i * 16 * actual_stride);
        write_u64_to_memory(memory, addr, value_low);
        write_u64_to_memory(memory, addr + 8, value_high);
    }
    
    0
}

/// 写入 u64 到内存
fn write_u64_to_memory(memory: &mut [u8], addr: u64, value: u64) {
    let addr = addr as usize;
    if addr + 8 > memory.len() {
        error!("Write out of bounds: addr=0x{:x}", addr);
        return;
    }
    let bytes = value.to_le_bytes();
    for (i, &byte) in bytes.iter().enumerate() {
        memory[addr + i] = byte;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emu::config::{BANK_NUM, BANK_SIZE, BankConfig};
    
    #[test]
    fn test_mvout_basic() {
        let mut memory = vec![0; BANK_NUM * BANK_SIZE];
        let mut banks: Vec<Vec<u8>> = (0..BANK_NUM).map(|_| vec![0; BANK_SIZE]).collect();
        let mut bank_configs = [BankConfig::default(); BANK_NUM];
        
        // 分配 bank 0 并写入数据（16 字节块格式）
        bank_configs[0].allocated = true;
        let test_values: [(u64, u64); 3] = [
            (0x1111111111111111u64, 0xAAAAAAAAAAAAAAAAu64),
            (0x2222222222222222u64, 0xBBBBBBBBBBBBBBBBu64),
            (0x3333333333333333u64, 0xCCCCCCCCCCCCCCCCu64),
        ];
        for (i, &(low, high)) in test_values.iter().enumerate() {
            let offset = i * 16;
            banks[0][offset..offset + 8].copy_from_slice(&low.to_le_bytes());
            banks[0][offset + 8..offset + 16].copy_from_slice(&high.to_le_bytes());
        }
        
        // MVOUT: 从 bank 存储到内存
        let out_addr = 0x200u64;
        let xs1 = 0 | (out_addr << 27);  // bank_id=0, mem_addr=0x200
        let xs2 = 3 | (1 << 10);         // depth=3, stride=1
        execute_mvout(xs1, xs2, &mut memory, &banks, &bank_configs);
        
        // 验证内存中的数据（每个块 16 字节）
        for (i, &(expected_low, expected_high)) in test_values.iter().enumerate() {
            let addr = (out_addr + (i * 16) as u64) as usize;
            let actual_low = u64::from_le_bytes(
                memory[addr..addr + 8].try_into().unwrap()
            );
            let actual_high = u64::from_le_bytes(
                memory[addr + 8..addr + 16].try_into().unwrap()
            );
            assert_eq!(actual_low, expected_low, "Low mismatch at index {}", i);
            assert_eq!(actual_high, expected_high, "High mismatch at index {}", i);
        }
    }
}
