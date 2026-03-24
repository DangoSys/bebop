use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// One 64-bit digest (16 hex chars) for an arbitrary byte slice (e.g. one bank).
/// Uses [`DefaultHasher`]; hash values are not guaranteed stable across Rust releases.
pub fn bank_hash64(data: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
