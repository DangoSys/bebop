//! Scratchpad bank 上的矩阵视图：i8（cols=1，行步长 16）与 acc i32（cols=4，行步长 64）。

/// i8 bank 每行固定 16 字节步长（与 `MATRIX_SIZE` 对齐方式一致）。
pub const I8_ROW_STRIDE: usize = 16;
/// acc bank（cols=4）每行 16 个 i32 → 64 字节。
pub const ACC_ROW_STRIDE: usize = 64;

/// n×n int8 子块（行主序，行步长 [`I8_ROW_STRIDE`]）。
pub fn read_i8_nn(banks: &[Vec<u8>], id: u64, n: usize) -> Vec<Vec<i8>> {
    let m = &banks[id as usize];
    let mut out = vec![vec![0i8; n]; n];
    for i in 0..n {
        for j in 0..n {
            out[i][j] = m[i * I8_ROW_STRIDE + j] as i8;
        }
    }
    out
}

/// `rows` 行 × `width` 列，列数 ≤ 16，行步长 [`I8_ROW_STRIDE`]（如 mul_warp16 的 K×16）。
pub fn read_i8_k_rows(banks: &[Vec<u8>], id: u64, rows: usize, width: usize) -> Vec<Vec<i8>> {
    let m = &banks[id as usize];
    let mut out = vec![vec![0i8; width]; rows];
    for i in 0..rows {
        for j in 0..width {
            out[i][j] = m[i * I8_ROW_STRIDE + j] as i8;
        }
    }
    out
}

/// n×n int32 累加 bank（`i * ACC_ROW_STRIDE + j * 4`）。
pub fn read_i32_nn(banks: &[Vec<u8>], id: u64, n: usize) -> Vec<Vec<i32>> {
    let b = &banks[id as usize];
    let mut out = vec![vec![0i32; n]; n];
    for i in 0..n {
        for j in 0..n {
            let off = i * ACC_ROW_STRIDE + j * 4;
            out[i][j] = i32::from_le_bytes(b[off..off + 4].try_into().unwrap());
        }
    }
    out
}

pub fn write_i32_nn(banks: &mut [Vec<u8>], id: u64, c: &[Vec<i32>], n: usize) {
    for i in 0..n {
        for j in 0..n {
            let off = i * ACC_ROW_STRIDE + j * 4;
            banks[id as usize][off..off + 4].copy_from_slice(&c[i][j].to_le_bytes());
        }
    }
}

/// 固定 16×16 acc 块，避免 Vec 分配。
pub fn read_i32_16x16(banks: &[Vec<u8>], id: u64) -> [[i32; 16]; 16] {
    let b = &banks[id as usize];
    let mut mat = [[0i32; 16]; 16];
    for i in 0..16 {
        for j in 0..16 {
            let off = i * ACC_ROW_STRIDE + j * 4;
            mat[i][j] = i32::from_le_bytes(b[off..off + 4].try_into().unwrap());
        }
    }
    mat
}

pub fn write_i32_16x16(banks: &mut [Vec<u8>], id: u64, m: &[[i32; 16]; 16]) {
    for i in 0..16 {
        for j in 0..16 {
            let off = i * ACC_ROW_STRIDE + j * 4;
            banks[id as usize][off..off + 4].copy_from_slice(&m[i][j].to_le_bytes());
        }
    }
}
