//! Shared memory helpers (POSIX `shm_open` + `mmap`).

pub mod layout;
pub mod posix;
mod worker;

pub use layout::{rpc_shutdown, BEBOP_SHM_SIZE};
pub use posix::ShmMap;
pub use worker::run as run_worker;
