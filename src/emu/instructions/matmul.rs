/// 矩阵运算指令实现
/// 包含 MUL_WARP16 (funct=32) 和 TRANSPOSE (funct=34)

use log::{error, info};
use super::super::config::{BANK_NUM, BANK_SIZE, MATRIX_SIZE};

/// MUL_WARP16 指令执行：16x16 矩阵乘法
/// 
/// 宏定义：bb_mul_warp16(op1_bank_id, op2_bank_id, wr_bank_id, iter, mode)
/// xs1 = BB_BANK0(op1_bank_id) | BB_BANK1(op2_bank_id) | BB_BANK2(wr_bank_id) | BB_RD0 | BB_RD1 | BB_WR
/// xs2 = FIELD(iter, 0, 9) | FIELD(mode, 10, 63)
/// 
/// # Arguments
/// * `xs1` - 包含三个 bank_id
/// * `xs2` - 包含 iter 和 mode
/// * `banks` - Bank 内存数组
pub fn execute_mul_warp16(xs1: u64, xs2: u64, banks: &mut [Vec<u8>]) -> u64 {
    // 解码 xs1：提取三个 bank_id
    let op1_bank_id = xs1 & 0x1F;              // bits 0-4 (BANK0)
    let op2_bank_id = (xs1 >> 5) & 0x1F;       // bits 5-9 (BANK1)
    let wr_bank_id = (xs1 >> 10) & 0x1F;       // bits 10-14 (BANK2)
    
    // 解码 xs2：提取 iter 和 mode
    let iter = xs2 & 0x3FF;                    // bits 0-9
    let _mode = (xs2 >> 10) & 0xFFFFFFFFFFFFF; // bits 10-63
    
    info!(
        "MUL_WARP16: op1={}, op2={}, wr={}, iter={}",
        op1_bank_id, op2_bank_id, wr_bank_id, iter
    );
    
    // 验证 bank 有效性
    if op1_bank_id >= BANK_NUM as u64 || op2_bank_id >= BANK_NUM as u64 || wr_bank_id >= BANK_NUM as u64 {
        error!("MUL_WARP16: Invalid bank_id");
        return 0;
    }
    
    // 从 bank 读取矩阵 A 和 B（16x16，每个元素 8 字节）
    let matrix_a = read_matrix_from_bank(banks, op1_bank_id);
    let matrix_b = read_matrix_from_bank(banks, op2_bank_id);
    
    // 计算矩阵乘法 C = A × B
    let mut result = [[0u64; MATRIX_SIZE]; MATRIX_SIZE];
    for i in 0..MATRIX_SIZE {
        for j in 0..MATRIX_SIZE {
            for k in 0..MATRIX_SIZE {
                result[i][j] = result[i][j].wrapping_add(
                    matrix_a[i][k].wrapping_mul(matrix_b[k][j])
                );
            }
        }
    }
    
    // 写入结果到 wr_bank
    write_matrix_to_bank(banks, wr_bank_id, &result);
    
    info!("MUL_WARP16: Computed C = A × B, stored in bank {}", wr_bank_id);
    0
}

/// TRANSPOSE 指令执行：矩阵转置
/// 
/// 宏定义：bb_transpose(op1_bank_id, wr_bank_id, iter, mode)
/// xs1 = BB_BANK0(op1_bank_id) | BB_BANK2(wr_bank_id) | BB_RD0 | BB_WR
/// xs2 = FIELD(iter, 0, 9) | FIELD(mode, 10, 63)
/// 
/// # Arguments
/// * `xs1` - 包含 op1_bank_id 和 wr_bank_id
/// * `xs2` - 包含 iter 和 mode
/// * `banks` - Bank 内存数组
pub fn execute_transpose(xs1: u64, xs2: u64, banks: &mut [Vec<u8>]) -> u64 {
    // 解码 xs1：提取 op1_bank_id 和 wr_bank_id
    let op1_bank_id = xs1 & 0x1F;        // bits 0-4 (BANK0)
    let wr_bank_id = (xs1 >> 10) & 0x1F; // bits 10-14 (BANK2)
    
    // 解码 xs2：提取 iter 和 mode
    let iter = xs2 & 0x3FF;              // bits 0-9
    let _mode = (xs2 >> 10) & 0xFFFFFFFFFFFFF; // bits 10-63
    
    info!(
        "TRANSPOSE: op1={}, wr={}, iter={}",
        op1_bank_id, wr_bank_id, iter
    );
    
    if op1_bank_id >= BANK_NUM as u64 || wr_bank_id >= BANK_NUM as u64 {
        error!("TRANSPOSE: Invalid bank_id");
        return 0;
    }
    
    // 从源 bank 读取矩阵
    let matrix = read_matrix_from_bank(banks, op1_bank_id);
    
    // 计算转置
    let mut transposed = [[0u64; MATRIX_SIZE]; MATRIX_SIZE];
    for i in 0..MATRIX_SIZE {
        for j in 0..MATRIX_SIZE {
            transposed[j][i] = matrix[i][j];
        }
    }
    
    // 写入目标 bank
    write_matrix_to_bank(banks, wr_bank_id, &transposed);
    
    info!("TRANSPOSE: Transposed matrix from bank {} to bank {}", op1_bank_id, wr_bank_id);
    0
}

/// 从 bank 读取 16x16 矩阵
fn read_matrix_from_bank(banks: &[Vec<u8>], bank_id: u64) -> [[u64; MATRIX_SIZE]; MATRIX_SIZE] {
    let mut matrix = [[0u64; MATRIX_SIZE]; MATRIX_SIZE];
    for i in 0..MATRIX_SIZE {
        for j in 0..MATRIX_SIZE {
            let offset = ((i * MATRIX_SIZE + j) * 8) as usize;
            if offset + 8 <= BANK_SIZE {
                matrix[i][j] = u64::from_le_bytes(
                    banks[bank_id as usize][offset..offset + 8]
                        .try_into().unwrap()
                );
            }
        }
    }
    matrix
}

/// 写入 16x16 矩阵到 bank
fn write_matrix_to_bank(banks: &mut [Vec<u8>], bank_id: u64, matrix: &[[u64; MATRIX_SIZE]; MATRIX_SIZE]) {
    for i in 0..MATRIX_SIZE {
        for j in 0..MATRIX_SIZE {
            let offset = ((i * MATRIX_SIZE + j) * 8) as usize;
            if offset + 8 <= BANK_SIZE {
                banks[bank_id as usize][offset..offset + 8]
                    .copy_from_slice(&matrix[i][j].to_le_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emu::config::{BANK_NUM, BANK_SIZE};
    
    #[test]
    fn test_mul_warp16_identity() {
        let mut banks: Vec<Vec<u8>> = (0..BANK_NUM).map(|_| vec![0; BANK_SIZE]).collect();
        
        // 初始化单位矩阵到 bank 0
        for i in 0..MATRIX_SIZE {
            let offset = (i * MATRIX_SIZE + i) * 8;
            banks[0][offset..offset + 8].copy_from_slice(&1u64.to_le_bytes());
        }
        
        // 初始化 2I 矩阵到 bank 1
        for i in 0..MATRIX_SIZE {
            let offset = (i * MATRIX_SIZE + i) * 8;
            banks[1][offset..offset + 8].copy_from_slice(&2u64.to_le_bytes());
        }
        
        // 执行矩阵乘法：I × 2I = 2I
        let xs1 = 0 | (1 << 5) | (2 << 10); // op1=0, op2=1, wr=2
        let xs2 = 16; // iter=16
        execute_mul_warp16(xs1, xs2, &mut banks);
        
        // 验证结果
        for i in 0..MATRIX_SIZE {
            for j in 0..MATRIX_SIZE {
                let offset = ((i * MATRIX_SIZE + j) * 8) as usize;
                let actual = u64::from_le_bytes(
                    banks[2][offset..offset + 8].try_into().unwrap()
                );
                let expected = if i == j { 2 } else { 0 };
                assert_eq!(actual, expected, "Mismatch at [{}][{}]", i, j);
            }
        }
    }
    
    #[test]
    fn test_transpose() {
        let mut banks: Vec<Vec<u8>> = (0..BANK_NUM).map(|_| vec![0; BANK_SIZE]).collect();
        
        // 初始化矩阵到 bank 0
        for i in 0..MATRIX_SIZE {
            for j in 0..MATRIX_SIZE {
                let offset = ((i * MATRIX_SIZE + j) * 8) as usize;
                let value = ((i * MATRIX_SIZE + j) as u64) + 1;
                banks[0][offset..offset + 8].copy_from_slice(&value.to_le_bytes());
            }
        }
        
        // 执行转置
        let xs1 = 0 | (1 << 10); // op1=0, wr=1
        let xs2 = 16; // iter=16
        execute_transpose(xs1, xs2, &mut banks);
        
        // 验证转置结果
        for i in 0..MATRIX_SIZE {
            for j in 0..MATRIX_SIZE {
                let offset = ((i * MATRIX_SIZE + j) * 8) as usize;
                let actual = u64::from_le_bytes(
                    banks[1][offset..offset + 8].try_into().unwrap()
                );
                let expected = ((j * MATRIX_SIZE + i) as u64) + 1;
                assert_eq!(actual, expected, "Transpose mismatch at [{}][{}]", i, j);
            }
        }
    }
}
