use std::collections::VecDeque;

pub const BASE: u64 = 0x1000_1000;
pub const SIZE: u64 = 0x1000;

#[derive(Default)]
pub struct Microphone {
    samples: VecDeque<i16>,
    ready: usize,
    phase: u64,
    enabled: bool,
}

impl Microphone {
    pub fn push_live(&mut self, samples: Vec<i16>) -> Result<(), &'static str> {
        if !self.enabled {
            return Ok(());
        }
        if self.samples.len() + samples.len() > 16_000 {
            return Err("microphone queue exceeds one second; guest cannot keep up");
        }
        self.samples.extend(samples);
        Ok(())
    }

    pub fn push(&mut self, samples: impl IntoIterator<Item = i16>) {
        self.samples.extend(samples);
    }

    pub fn tick(&mut self, ns: u64) {
        if self.enabled {
            let elapsed = self.phase as u128 + ns as u128 * 16_000;
            self.ready = (self.ready as u128 + elapsed / 1_000_000_000).min(self.samples.len() as u128) as usize;
            self.phase = (elapsed % 1_000_000_000) as u64;
        }
    }

    pub fn load(&mut self, offset: u64, size: usize) -> Option<u64> {
        match (offset, size) {
            (0, 4) => Some(self.ready as u64),
            (4, 2) if self.ready != 0 => {
                self.ready -= 1;
                Some(self.samples.pop_front().unwrap() as u16 as u64)
            }
            (8, 4) => Some(16_000), // Mono signed PCM16, 16 kHz.
            (12, 4) => Some(self.enabled.into()),
            (16, 4) => Some(self.samples.is_empty().into()),
            _ => None,
        }
    }

    pub fn store(&mut self, offset: u64, size: usize, value: u64) -> bool {
        match (offset, size, value) {
            (12, 4, 0 | 1) => self.enabled = value != 0,
            _ => return false,
        }
        true
    }
}
