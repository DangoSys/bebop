use log::{error, info};

use super::super::config::{BANK_NUM, MATRIX_SIZE};
use super::matmul::{read_matrix_from_bank, write_matrix_to_bank};

pub fn execute_transpose(xs1: u64, xs2: u64, banks: &mut [Vec<u8>]) -> u64 {
    let op1_bank_id = xs1 & 0x1F;
    let wr_bank_id = (xs1 >> 10) & 0x1F;
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
