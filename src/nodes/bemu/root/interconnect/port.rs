use super::fabric::{Fabric, Transfer, COMPLETION_SLOTS};
use rvsim::bus::{BusError, Width};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

pub const BASE: u64 = 0x6200_0000;
pub const SIZE: u64 = 0x1000;
pub const BUFFER_BASE: u64 = 0x6210_0000;
pub const BUFFER_BYTES: u64 = 0x10_0000;

pub(crate) struct Port {
    pub(crate) fabric: Arc<Fabric>,
    chip: usize,
    memory_bytes: u64,
    buffer_address: u64,
    source: u64,
    destination_chip: usize,
    destination: u64,
    bytes: u64,
    tag: u64,
    pub(crate) status: Arc<AtomicU64>,
}

impl Port {
    pub(crate) fn new(fabric: Arc<Fabric>, chip: usize, memory_bytes: u64, buffer_address: u64) -> Self {
        Self {
            fabric,
            chip,
            memory_bytes,
            buffer_address,
            source: 0,
            destination_chip: 0,
            destination: 0,
            bytes: 0,
            tag: 0,
            status: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(crate) fn read(&self, offset: u64, width: Width) -> Result<u64, BusError> {
        if width != Width::Double {
            return Err(BusError);
        }
        match offset {
            0 => Ok(self.chip as u64),
            8 => Ok(self.fabric.chips as u64),
            112 => Ok(self.memory_bytes),
            128 => Ok(self.buffer_address),
            136 => Ok(BUFFER_BYTES),
            120 => Ok(COMPLETION_SLOTS as u64),
            16 => Ok(self.source),
            24 => Ok(self.destination_chip as u64),
            32 => Ok(self.destination),
            40 => Ok(self.bytes),
            48 => Ok(self.tag),
            64 => Ok(self.status.load(Ordering::Acquire)),
            72 | 80 | 88 | 96 => {
                let state = self.fabric.state.lock().expect("interconnect poisoned");
                if offset == 72 {
                    return Ok(state.receipts[self.chip].len() as u64);
                }
                let event = state.receipts[self.chip].front().ok_or(BusError)?;
                match offset {
                    80 => Ok(event.source as u64),
                    88 => Ok(event.tag),
                    96 => Ok(event.bytes),
                    _ => unreachable!(),
                }
            }
            _ => Err(BusError),
        }
    }

    pub(crate) fn write(&mut self, offset: u64, width: Width, value: u64) -> Result<(), BusError> {
        if width != Width::Double {
            return Err(BusError);
        }
        if offset == 104 {
            if value != 1 {
                return Err(BusError);
            }
            self.fabric.state.lock().expect("interconnect poisoned").receipts[self.chip]
                .pop_front()
                .ok_or(BusError)?;
            self.fabric.ready.notify_one();
            return Ok(());
        }
        if self.status.load(Ordering::Acquire) == 1 {
            return Err(BusError);
        }
        match offset {
            16 => self.source = value,
            24 => self.destination_chip = value.try_into().map_err(|_| BusError)?,
            32 => self.destination = value,
            40 => self.bytes = value,
            48 => self.tag = value,
            56 => {
                if value != 1 || self.bytes == 0 || self.destination_chip >= self.fabric.chips {
                    return Err(BusError);
                }
                let mut state = self.fabric.state.lock().expect("interconnect poisoned");
                if state.stopped {
                    return Err(BusError);
                }
                self.status.store(1, Ordering::Release);
                state.pending.push_back(Transfer {
                    source_chip: self.chip,
                    source: self.source,
                    destination_chip: self.destination_chip,
                    destination: self.destination,
                    bytes: self.bytes,
                    tag: self.tag,
                    status: Arc::clone(&self.status),
                });
                self.fabric.ready.notify_one();
            }
            _ => return Err(BusError),
        }
        Ok(())
    }
}
