//! Shared memory helpers (POSIX `shm_open` + `mmap`).

pub mod layout;
pub mod posix;
mod smoke;
mod worker;

pub use layout::{rpc_shutdown, BEBOP_SHM_SIZE};
pub use posix::ShmMap;
pub use smoke::run as run_smoke;
pub use worker::run as run_worker;
