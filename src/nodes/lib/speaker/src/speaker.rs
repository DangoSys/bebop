use std::collections::VecDeque;

pub const BASE: u64 = 0x1000_2000;
pub const SIZE: u64 = 0x1000;
pub const CAPACITY: usize = 16_000;

#[derive(Default)]
pub struct Speaker {
    queue: VecDeque<i16>,
    played: Vec<i16>,
    phase: u64,
    enabled: bool,
    underrun: bool,
}

impl Speaker {
    pub fn tick(&mut self, ns: u64) {
        if self.enabled {
            let elapsed = self.phase as u128 + ns as u128 * 16_000;
            let count = elapsed / 1_000_000_000;
            self.phase = (elapsed % 1_000_000_000) as u64;
            self.underrun |= count > self.queue.len() as u128;
            let ready = count.min(self.queue.len() as u128) as usize;
            self.played.extend(self.queue.drain(..ready));
        }
    }

    pub fn take_samples(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.played)
    }

    pub fn load(&self, offset: u64, size: usize) -> Option<u64> {
        match (offset, size) {
            (0, 4) => Some(self.queue.len() as u64),
            (8, 4) => Some(16_000), // Mono signed PCM16, 16 kHz.
            (12, 4) => Some(self.enabled.into()),
            (16, 4) => Some(CAPACITY as u64),
            (20, 4) => Some(self.underrun.into()),
            _ => None,
        }
    }

    pub fn store(&mut self, offset: u64, size: usize, value: u64) -> bool {
        match (offset, size, value) {
            (4, 2, _) if self.queue.len() < CAPACITY => self.queue.push_back(value as i16),
            (12, 4, 0 | 1) => self.enabled = value != 0,
            (20, 4, 1) => self.underrun = false,
            _ => return false,
        }
        true
    }
}
