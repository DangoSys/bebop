//! TLink: what a hart offers its hardware threads and every DMA master inside it.

use crate::{clint::Clint, ddr::Ddr, guest::ThreadMemory, hlink::Peripherals, tasks::Tasks, timing::HartTime};
use std::sync::{Arc, Mutex, OnceLock};

/// The hart side of the link: its id and time base, the memory a DMA reaches and the timing
/// of its transfers, the storage the hart keeps for its threads, and the task channel. The
/// hart builds it; a thread is built from it.
pub struct TLink {
    pub hart_id: usize,
    pub clint: Arc<Clint>,
    pub platform: Arc<Mutex<dyn Peripherals>>,
    pub memory: Arc<Ddr>,
    pub transfers: Mutex<HartTime>,
    pub thread_memory: OnceLock<Arc<dyn ThreadMemory>>,
    pub tasks: Tasks,
}
