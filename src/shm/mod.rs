//! Shared memory helpers (POSIX `shm_open` + `mmap`).

pub mod posix;
mod smoke;

pub use smoke::run as run_smoke;
