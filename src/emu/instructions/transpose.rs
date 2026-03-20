use log::{error, info};

use super::super::configs::config::{BankConfig, BANK_NUM, MATRIX_SIZE};
use super::matmul::{read_matrix_from_bank, write_matrix_to_bank};

pub fn execute_transpose(
    xs1: u64,
    xs2: u64,
    banks: &mut [Vec<u8>],
    bank_configs: &[BankConfig],
) -> u64 {
    let op1_bank_id = xs1 & 0xFF; // bits 0-7 (BB_BANK0)
    let wr_bank_id = (xs1 >> 16) & 0xFF; // bits 16-23 (BB_BANK2)
    let iter = xs2 & 0x3FF;
    let _mode = (xs2 >> 10) & 0xFFFFFFFFFFFFF;

    info!(
        "TRANSPOSE: op1={}, wr={}, iter={}",
        op1_bank_id, wr_bank_id, iter
    );
    if op1_bank_id >= BANK_NUM as u64 || wr_bank_id >= BANK_NUM as u64 {
        error!("TRANSPOSE: Invalid bank_id");
        return 0;
    }

    let op1_cols = bank_configs[op1_bank_id as usize].cols;
    let wr_cols = bank_configs[wr_bank_id as usize].cols;
    let n = usize::min(iter as usize, MATRIX_SIZE);

    // vecunit transpose path for int8 banks (cols=1)
    if op1_cols == 1 && wr_cols == 1 {
        for i in 0..n {
            for j in 0..n {
                let src = i * 16 + j;
                let dst = j * 16 + i;
                banks[wr_bank_id as usize][dst] = banks[op1_bank_id as usize][src];
            }
        }
        info!(
            "TRANSPOSE(vecunit): transposed int8 matrix from bank {} to bank {}, n={}",
            op1_bank_id, wr_bank_id, n
        );
        return 0;
    }

    // vecunit accumulator transpose path for int32 banks (cols=4)
    if op1_cols == 4 && wr_cols == 4 {
        for i in 0..n {
            for j in 0..n {
                let src_off = i * 64 + j * 4;
                let dst_off = j * 64 + i * 4;
                let v = i32::from_le_bytes(
                    banks[op1_bank_id as usize][src_off..src_off + 4]
                        .try_into()
                        .unwrap(),
                );
                banks[wr_bank_id as usize][dst_off..dst_off + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        info!(
            "TRANSPOSE(vecunit): transposed int32 matrix from bank {} to bank {}, n={}",
            op1_bank_id, wr_bank_id, n
        );
        return 0;
    }

    // Legacy fallback
    let matrix = read_matrix_from_bank(banks, op1_bank_id);
    let mut transposed = [[0u64; MATRIX_SIZE]; MATRIX_SIZE];
    for i in 0..MATRIX_SIZE {
        for j in 0..MATRIX_SIZE {
            transposed[j][i] = matrix[i][j];
        }
    }
    write_matrix_to_bank(banks, wr_bank_id, &transposed);
    info!(
        "TRANSPOSE: Transposed matrix from bank {} to bank {}",
        op1_bank_id, wr_bank_id
    );
    0
}
