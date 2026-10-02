#![allow(dead_code)]

use super::instruction::TrackedBanks;

const I8_ROW_STRIDE: usize = 16;
const I32_ROW_STRIDE: usize = 64;

pub fn read_i8_nn(banks: &TrackedBanks<'_>, p: usize, n: usize) -> Vec<Vec<i8>> {
    (0..n)
        .map(|i| (0..n).map(|j| banks[p][i * I8_ROW_STRIDE + j] as i8).collect())
        .collect()
}

pub fn read_i8_nn_at(banks: &TrackedBanks<'_>, p: usize, base: usize, n: usize) -> Vec<Vec<i8>> {
    (0..n)
        .map(|i| (0..n).map(|j| banks[p][(base + i) * I8_ROW_STRIDE + j] as i8).collect())
        .collect()
}

pub fn read_i8_k_rows(banks: &TrackedBanks<'_>, p: usize, rows: usize, width: usize) -> Vec<Vec<i8>> {
    (0..rows)
        .map(|i| (0..width).map(|j| banks[p][i * I8_ROW_STRIDE + j] as i8).collect())
        .collect()
}

pub fn read_i32_nn(banks: &TrackedBanks<'_>, p: usize, n: usize) -> Vec<Vec<i32>> {
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

pub fn read_i32_nn_at(banks: &TrackedBanks<'_>, p: usize, base: usize, n: usize) -> Vec<Vec<i32>> {
    (0..n)
        .map(|i| {
            (0..n)
                .map(|j| {
                    let off = (base + i) * I32_ROW_STRIDE + j * 4;
                    i32::from_le_bytes(banks[p][off..off + 4].try_into().unwrap())
                })
                .collect()
        })
        .collect()
}

pub fn write_i32_nn(banks: &mut TrackedBanks<'_>, p: usize, mat: &[Vec<i32>], n: usize) {
    for (i, row) in mat.iter().enumerate().take(n) {
        for (group, lanes) in row[..n].chunks(4).enumerate() {
            let mut data = [0; 16];
            for (lane, value) in lanes.iter().enumerate() {
                data[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
            banks.write_row(p, i * 4 + group, data, (1u32 << (lanes.len() * 4)) - 1);
        }
    }
}

pub fn read_i32_nn_groups(banks: &TrackedBanks<'_>, ps: &[usize], n: usize) -> Vec<Vec<i32>> {
    (0..n)
        .map(|i| {
            (0..n)
                .map(|j| {
                    let group = j / 4;
                    let lane = j % 4;
                    let off = i * I8_ROW_STRIDE + lane * 4;
                    i32::from_le_bytes(banks[ps[group]][off..off + 4].try_into().unwrap())
                })
                .collect()
        })
        .collect()
}

pub fn read_i32_nn_groups_at(banks: &TrackedBanks<'_>, ps: &[usize], base: usize, n: usize) -> Vec<Vec<i32>> {
    (0..n)
        .map(|i| {
            (0..n)
                .map(|j| {
                    let off = (base + i) * I8_ROW_STRIDE + (j % 4) * 4;
                    i32::from_le_bytes(banks[ps[j / 4]][off..off + 4].try_into().unwrap())
                })
                .collect()
        })
        .collect()
}

pub fn write_i32_nn_groups(banks: &mut TrackedBanks<'_>, ps: &[usize], mat: &[Vec<i32>], n: usize) {
    for (i, row) in mat.iter().enumerate().take(n) {
        for (group, lanes) in row[..n].chunks(4).enumerate() {
            let mut data = [0; 16];
            for (lane, value) in lanes.iter().enumerate() {
                data[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
            banks.write_row(ps[group], i, data, (1u32 << (lanes.len() * 4)) - 1);
        }
    }
}

pub fn write_i32_nn_groups_at(banks: &mut TrackedBanks<'_>, ps: &[usize], base: usize, mat: &[Vec<i32>], n: usize) {
    for (i, row) in mat.iter().enumerate().take(n) {
        for (group, lanes) in row[..n].chunks(4).enumerate() {
            let mut data = [0; 16];
            for (lane, value) in lanes.iter().enumerate() {
                data[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
            banks.write_row(ps[group], base + i, data, (1u32 << (lanes.len() * 4)) - 1);
        }
    }
}
