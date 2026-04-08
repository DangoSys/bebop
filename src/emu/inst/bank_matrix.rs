//! Matrix views on scratchpad banks: i8 (cols=1, row stride 16) and acc i32 (cols=4, row stride 64).
//! `p` is the **physical bank index** (resolved by `decode::pbank` before passing in).

/// i8 bank uses a fixed 16-byte row stride (aligned with `MATRIX_SIZE` layout).
pub const I8_ROW_STRIDE: usize = 16;
/// acc bank (cols=4): 16 i32 values per row -> 64 bytes.
pub const ACC_ROW_STRIDE: usize = 64;

/// n×n int8 tile (row-major, row stride [`I8_ROW_STRIDE`]).
pub fn read_i8_nn(banks: &[Vec<u8>], p: usize, n: usize) -> Vec<Vec<i8>> {
    let m = &banks[p];
    let mut out = vec![vec![0i8; n]; n];
    for i in 0..n {
        for j in 0..n {
            out[i][j] = m[i * I8_ROW_STRIDE + j] as i8;
        }
    }
    out
}

/// `rows` x `width`, width <= 16, row stride [`I8_ROW_STRIDE`] (e.g. Kx16 in mul_warp16).
// pub fn read_i8_k_rows(banks: &[Vec<u8>], p: usize, rows: usize, width: usize) -> Vec<Vec<i8>> {
//     let m = &banks[p];
//     let mut out = vec![vec![0i8; width]; rows];
//     for i in 0..rows {
//         for j in 0..width {
//             out[i][j] = m[i * I8_ROW_STRIDE + j] as i8;
//         }
//     }
//     out
// }

/// n×n int32 accumulator bank (`i * ACC_ROW_STRIDE + j * 4`).
pub fn read_i32_nn(banks: &[Vec<u8>], p: usize, n: usize) -> Vec<Vec<i32>> {
    let b = &banks[p];
    let mut out = vec![vec![0i32; n]; n];
    for i in 0..n {
        for j in 0..n {
            let off = i * ACC_ROW_STRIDE + j * 4;
            out[i][j] = i32::from_le_bytes(b[off..off + 4].try_into().unwrap());
        }
    }
    out
}

pub fn write_i32_nn(banks: &mut [Vec<u8>], p: usize, c: &[Vec<i32>], n: usize) {
    for i in 0..n {
        for j in 0..n {
            let off = i * ACC_ROW_STRIDE + j * 4;
            banks[p][off..off + 4].copy_from_slice(&c[i][j].to_le_bytes());
        }
    }
}

/// Fixed 16x16 acc tile to avoid Vec allocation.
pub fn read_i32_16x16(banks: &[Vec<u8>], p: usize) -> [[i32; 16]; 16] {
    let b = &banks[p];
    let mut mat = [[0i32; 16]; 16];
    for i in 0..16 {
        for j in 0..16 {
            let off = i * ACC_ROW_STRIDE + j * 4;
            mat[i][j] = i32::from_le_bytes(b[off..off + 4].try_into().unwrap());
        }
    }
    mat
}

pub fn write_i32_16x16(banks: &mut [Vec<u8>], p: usize, m: &[[i32; 16]; 16]) {
    for i in 0..16 {
        for j in 0..16 {
            let off = i * ACC_ROW_STRIDE + j * 4;
            banks[p][off..off + 4].copy_from_slice(&m[i][j].to_le_bytes());
        }
    }
}
