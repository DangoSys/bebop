use super::super::configs::config::{BankConfig, BANK_NUM, BANK_SIZE, MATRIX_SIZE};
use log::{error, info};

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
pub fn execute_mul_warp16(
    xs1: u64,
    xs2: u64,
    banks: &mut [Vec<u8>],
    bank_configs: &[BankConfig],
) -> u64 {
    // 解码 xs1：提取三个 bank_id
    let op1_bank_id = xs1 & 0xFF; // bits 0-7 (BB_BANK0)
    let op2_bank_id = (xs1 >> 8) & 0xFF; // bits 8-15 (BB_BANK1)
    let wr_bank_id = (xs1 >> 16) & 0xFF; // bits 16-23 (BB_BANK2)

    // 解码 xs2：提取 iter 和 mode
    let iter = xs2 & 0x3FF; // bits 0-9
    let _mode = (xs2 >> 10) & 0xFFFFFFFFFFFFF; // bits 10-63

    info!(
        "MUL_WARP16: op1={}, op2={}, wr={}, iter={}",
        op1_bank_id, op2_bank_id, wr_bank_id, iter
    );

    // 验证 bank 有效性
    if op1_bank_id >= BANK_NUM as u64
        || op2_bank_id >= BANK_NUM as u64
        || wr_bank_id >= BANK_NUM as u64
    {
        error!("MUL_WARP16: Invalid bank_id");
        return 0;
    }

    // Buckyball vecunit path: op banks typically cols=1(int8 rows), acc bank cols=4(int32 rows)
    let op1_cols = bank_configs[op1_bank_id as usize].cols;
    let op2_cols = bank_configs[op2_bank_id as usize].cols;
    let wr_cols = bank_configs[wr_bank_id as usize].cols;
    if op1_cols == 1 && op2_cols == 1 && wr_cols == 4 {
        let n = usize::min(iter as usize, MATRIX_SIZE);
        let a_t = read_i8_matrix_from_bank(banks, op1_bank_id, n);
        let b = read_i8_matrix_from_bank(banks, op2_bank_id, n);
        let mut c = read_i32_matrix_from_bank(banks, wr_bank_id, n);

        // op1 is pre-transposed by bb_transpose in bb-tests:
        // A_t[k][i] corresponds to original A[i][k].
        for i in 0..n {
            for j in 0..n {
                let mut acc = c[i][j];
                for k in 0..n {
                    acc = acc.wrapping_add((a_t[k][i] as i32).wrapping_mul(b[k][j] as i32));
                }
                c[i][j] = acc;
            }
        }
        write_i32_matrix_to_bank(banks, wr_bank_id, &c, n);
        info!(
            "MUL_WARP16(vecunit): C += A*B (int8xint8->int32), n={}, wr={}",
            n, wr_bank_id
        );
        return 0;
    }

    // Legacy fallback (u64 matrix multiply) for existing simplified tests.
    let matrix_a = read_matrix_from_bank(banks, op1_bank_id);
    let matrix_b = read_matrix_from_bank(banks, op2_bank_id);
    let mut result = [[0u64; MATRIX_SIZE]; MATRIX_SIZE];
    for i in 0..MATRIX_SIZE {
        for j in 0..MATRIX_SIZE {
            for k in 0..MATRIX_SIZE {
                result[i][j] =
                    result[i][j].wrapping_add(matrix_a[i][k].wrapping_mul(matrix_b[k][j]));
            }
        }
    }
    write_matrix_to_bank(banks, wr_bank_id, &result);

    info!(
        "MUL_WARP16: Computed C = A × B, stored in bank {}",
        wr_bank_id
    );
    0
}

fn read_i8_matrix_from_bank(
    banks: &[Vec<u8>],
    bank_id: u64,
    n: usize,
) -> [[i8; MATRIX_SIZE]; MATRIX_SIZE] {
    let mut matrix = [[0i8; MATRIX_SIZE]; MATRIX_SIZE];
    for i in 0..n {
        for j in 0..n {
            matrix[i][j] = banks[bank_id as usize][i * 16 + j] as i8;
        }
    }
    matrix
}

fn read_i32_matrix_from_bank(
    banks: &[Vec<u8>],
    bank_id: u64,
    n: usize,
) -> [[i32; MATRIX_SIZE]; MATRIX_SIZE] {
    let mut matrix = [[0i32; MATRIX_SIZE]; MATRIX_SIZE];
    for i in 0..n {
        for j in 0..n {
            let off = i * 64 + j * 4;
            matrix[i][j] =
                i32::from_le_bytes(banks[bank_id as usize][off..off + 4].try_into().unwrap());
        }
    }
    matrix
}

fn write_i32_matrix_to_bank(
    banks: &mut [Vec<u8>],
    bank_id: u64,
    matrix: &[[i32; MATRIX_SIZE]; MATRIX_SIZE],
    n: usize,
) {
    for i in 0..n {
        for j in 0..n {
            let off = i * 64 + j * 4;
            banks[bank_id as usize][off..off + 4].copy_from_slice(&matrix[i][j].to_le_bytes());
        }
    }
}

pub(super) fn read_matrix_from_bank(
    banks: &[Vec<u8>],
    bank_id: u64,
) -> [[u64; MATRIX_SIZE]; MATRIX_SIZE] {
    let mut matrix = [[0u64; MATRIX_SIZE]; MATRIX_SIZE];
    for i in 0..MATRIX_SIZE {
        for j in 0..MATRIX_SIZE {
            let offset = ((i * MATRIX_SIZE + j) * 8) as usize;
            if offset + 8 <= BANK_SIZE {
                matrix[i][j] = u64::from_le_bytes(
                    banks[bank_id as usize][offset..offset + 8]
                        .try_into()
                        .unwrap(),
                );
            }
        }
    }
    matrix
}

pub(super) fn write_matrix_to_bank(
    banks: &mut [Vec<u8>],
    bank_id: u64,
    matrix: &[[u64; MATRIX_SIZE]; MATRIX_SIZE],
) {
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
