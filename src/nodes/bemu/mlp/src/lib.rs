//! Memory-level parallelism in BEMU: the caches that keep a CPU's accesses from each waiting on
//! the hart's memory system.
mod direct_mapped;

pub use direct_mapped::DirectMapped;
