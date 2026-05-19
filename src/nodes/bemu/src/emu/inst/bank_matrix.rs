const I8_ROW_STRIDE: usize = 16;
const I32_ROW_STRIDE: usize = 64;

pub fn read_i8_nn(banks: &[Vec<u8>], p: usize, n: usize) -> Vec<Vec<i8>> {
    (0..n)
        .map(|i| (0..n).map(|j| banks[p][i * I8_ROW_STRIDE + j] as i8).collect())
        .collect()
}

pub fn read_i8_k_rows(banks: &[Vec<u8>], p: usize, rows: usize, width: usize) -> Vec<Vec<i8>> {
    (0..rows)
        .map(|i| (0..width).map(|j| banks[p][i * I8_ROW_STRIDE + j] as i8).collect())
        .collect()
}

pub fn read_i32_nn(banks: &[Vec<u8>], p: usize, n: usize) -> Vec<Vec<i32>> {
    (0..n)
        .map(|i| {
            (0..n)
                .map(|j| {
                    let off = i * I32_ROW_STRIDE + j * 4;
                    i32::from_le_bytes(banks[p][off..off + 4].try_into().unwrap())
                })
                .collect()
        })
        .collect()
}

pub fn write_i32_nn(banks: &mut [Vec<u8>], p: usize, mat: &[Vec<i32>], n: usize) {
    for (i, row) in mat.iter().enumerate().take(n) {
        for (j, v) in row.iter().enumerate().take(n) {
            let off = i * I32_ROW_STRIDE + j * 4;
            banks[p][off..off + 4].copy_from_slice(&v.to_le_bytes());
        }
    }
}
