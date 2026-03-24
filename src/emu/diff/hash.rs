use crate::emu::bank::BankConfig;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

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
