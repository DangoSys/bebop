use super::port;

pub(super) const COMPLETION_SLOTS: usize = 64;

use crate::root::{chip::Chip, platform::DRAM_BASE};
use std::{
    collections::VecDeque,
    ops::Range,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
};

pub(super) struct Transfer {
    pub(super) source_chip: usize,
    pub(super) source: u64,
    pub(super) destination_chip: usize,
    pub(super) destination: u64,
    pub(super) bytes: u64,
    pub(super) tag: u64,
    pub(super) status: Arc<AtomicU64>,
}

pub(super) struct Receipt {
    pub(super) source: usize,
    pub(super) tag: u64,
    pub(super) bytes: u64,
}

pub(super) struct State {
    pub(super) pending: VecDeque<Transfer>,
    pub(super) receipts: Vec<VecDeque<Receipt>>,
    pub(super) stopped: bool,
}

pub struct Fabric {
    pub(super) chips: usize,
    pub(super) state: Mutex<State>,
    pub(super) ready: Condvar,
}

fn memory_range(address: u64, bytes: u64, capacity: usize) -> Result<Range<usize>, String> {
    let start = address.checked_sub(DRAM_BASE).ok_or("DMA address below DDR")?;
    let end = start.checked_add(bytes).ok_or("DMA address overflow")?;
    if end > capacity as u64 {
        return Err("DMA range exceeds chip DDR".into());
    }
    Ok(start as usize..end as usize)
}

impl Fabric {
    pub fn connect(chips: &[Chip]) -> Arc<Self> {
        assert!(!chips.is_empty(), "interconnect requires chips");
        let fabric = Arc::new(Self {
            chips: chips.len(),
            ready: Condvar::new(),
            state: Mutex::new(State {
                pending: VecDeque::new(),
                receipts: (0..chips.len()).map(|_| VecDeque::new()).collect(),
                stopped: false,
            }),
        });
        for (id, chip) in chips.iter().enumerate() {
            let mut platform = chip.platform.lock().expect("BEMU platform poisoned");
            assert!(platform.interconnect.is_none(), "chip already connected");
            let buffer = platform
                .pages
                .lock()
                .expect("DDR page pool poisoned")
                .reserve_interconnect(port::BUFFER_BYTES);
            let memory_bytes = platform.memory.len() as u64;
            platform.interconnect = Some(port::Port::new(Arc::clone(&fabric), id, memory_bytes, buffer));
        }
        fabric
    }

    pub fn stop(&self) {
        let mut state = self.state.lock().expect("interconnect poisoned");
        state.stopped = true;
        for transfer in state.pending.drain(..) {
            transfer.status.store(3, Ordering::Release);
        }
        self.ready.notify_all();
    }

    pub fn serve(&self, chips: &[Chip]) -> Result<(), String> {
        assert_eq!(self.chips, chips.len());
        loop {
            let transfer = {
                let mut state = self.state.lock().expect("interconnect poisoned");
                loop {
                    if state.stopped {
                        return Ok(());
                    }
                    if let Some(index) = state
                        .pending
                        .iter()
                        .position(|transfer| state.receipts[transfer.destination_chip].len() < COMPLETION_SLOTS)
                    {
                        break state.pending.remove(index).expect("pending DMA");
                    }
                    state = self.ready.wait(state).expect("interconnect poisoned");
                }
            };
            let result = self.copy(chips, &transfer);
            match result {
                Ok(()) => {
                    self.state.lock().expect("interconnect poisoned").receipts[transfer.destination_chip].push_back(
                        Receipt {
                            source: transfer.source_chip,
                            tag: transfer.tag,
                            bytes: transfer.bytes,
                        },
                    );
                    transfer.status.store(2, Ordering::Release);
                }
                Err(error) => {
                    transfer.status.store(3, Ordering::Release);
                    self.stop();
                    return Err(format!("chip {} DMA: {error}", transfer.source_chip));
                }
            }
        }
    }

    fn copy(&self, chips: &[Chip], transfer: &Transfer) -> Result<(), String> {
        let source = &chips[transfer.source_chip].memory;
        let target = &chips[transfer.destination_chip].memory;
        memory_range(transfer.source, transfer.bytes, source.len())?;
        memory_range(transfer.destination, transfer.bytes, target.len())?;
        let mut bytes = vec![0; transfer.bytes as usize];
        source
            .read_buffer(transfer.source, &mut bytes)
            .map_err(|_| "DMA source range".to_string())?;
        target
            .write_buffer(transfer.destination, &bytes)
            .map_err(|_| "DMA destination range".to_string())?;
        Ok(())
    }
}
