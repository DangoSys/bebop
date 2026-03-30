use crate::emu::bank::BankConfig;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// FNV-1a 64-bit offset basis (stable across Rust / C / Verilog cosim).
pub const FNV1A64_OFFSET: u64 = 14695981039346656037;
/// FNV-1a 64-bit prime.
pub const FNV1A64_PRIME: u64 = 1099511628211;

/// Deterministic 64-bit FNV-1a over `data` (per-bank building block; aggregate uses the same FNV constants).
#[allow(dead_code)]
pub fn fnv1a64(data: &[u8]) -> u64 {
    let mut h = FNV1A64_OFFSET;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(FNV1A64_PRIME);
    }
    h
}

/// Same bank selection as [`bank_hash`], then fold banks into one 64-bit digest (FNV stream:
/// per included bank: index u32 LE, length u32 LE, then bytes). Returns `0` iff no bank included.
pub fn cosim_aggregate_banks_digest(banks: &[Vec<u8>], cfg: &[BankConfig], all_banks: bool) -> u64 {
    let mut h = FNV1A64_OFFSET;
    let mut any = false;
    for (i, b) in banks.iter().enumerate() {
        if all_banks || cfg.get(i).map(|c| c.allocated).unwrap_or(false) {
            any = true;
            for byte in (i as u32).to_le_bytes() {
                h ^= byte as u64;
                h = h.wrapping_mul(FNV1A64_PRIME);
            }
            for byte in (b.len() as u32).to_le_bytes() {
                h ^= byte as u64;
                h = h.wrapping_mul(FNV1A64_PRIME);
            }
            for &byte in b.iter() {
                h ^= byte as u64;
                h = h.wrapping_mul(FNV1A64_PRIME);
            }
        }
    }
    if any {
        h
    } else {
        0
    }
}

/// One 64-bit digest (16 hex chars) for an arbitrary byte slice (e.g. one bank).
/// Uses [`DefaultHasher`]; hash values are not guaranteed stable across Rust releases.
pub fn single_bank_hash64(data: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn bank_hash(banks: &[Vec<u8>], cfg: &[BankConfig], all_banks: bool) -> String {
    let mut out: Vec<String> = Vec::new();
    for (i, b) in banks.iter().enumerate() {
        if all_banks || cfg.get(i).map(|c| c.allocated).unwrap_or(false) {
            out.push(format!("b{i}={}", single_bank_hash64(b)));
        }
    }
    if out.is_empty() {
        return "no-banks".to_string();
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a64_empty_is_offset() {
        assert_eq!(fnv1a64(b""), FNV1A64_OFFSET);
    }

    #[test]
    fn fnv1a64_golden() {
        assert_eq!(fnv1a64(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn cosim_aggregate_empty() {
        let banks: Vec<Vec<u8>> = (0..4).map(|_| vec![0u8; 8]).collect();
        let cfg = [BankConfig::default(); 4];
        assert_eq!(cosim_aggregate_banks_digest(&banks, &cfg, false), 0);
    }

    #[test]
    fn cosim_aggregate_golden_one_bank() {
        let mut banks: Vec<Vec<u8>> = (0..2).map(|_| vec![0u8; 4]).collect();
        banks[0] = vec![1, 2, 3, 4];
        let mut cfg = [BankConfig::default(); 2];
        cfg[0].allocated = true;
        let d = cosim_aggregate_banks_digest(&banks, &cfg, false);
        let mut h = FNV1A64_OFFSET;
        for b in 0u32.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(FNV1A64_PRIME);
        }
        for b in 4u32.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(FNV1A64_PRIME);
        }
        for &b in &[1u8, 2, 3, 4] {
            h ^= b as u64;
            h = h.wrapping_mul(FNV1A64_PRIME);
        }
        assert_eq!(d, h);
    }

    #[test]
    fn cosim_aggregate_all_banks_includes_unallocated() {
        let banks: Vec<Vec<u8>> = vec![vec![7], vec![8]];
        let cfg = [BankConfig::default(); 2];
        let d_all = cosim_aggregate_banks_digest(&banks, &cfg, true);
        assert_ne!(d_all, 0);
        assert_eq!(cosim_aggregate_banks_digest(&banks, &cfg, false), 0);
    }
}
