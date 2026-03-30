//! Shared memory helpers (POSIX `shm_open` + `mmap`).

pub mod layout;
pub mod posix;
pub mod protocol;

pub use layout::{rpc_shutdown, CosimShutdown, BEBOP_SHM_SIZE};
pub use posix::ShmMap;
