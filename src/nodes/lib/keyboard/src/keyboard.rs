use std::collections::VecDeque;

pub const BASE: u64 = 0x1000_3000;
pub const SIZE: u64 = 0x1000;

#[derive(Default)]
pub struct Keyboard {
    events: VecDeque<u32>,
}

impl Keyboard {
    pub fn push(&mut self, code: u16, pressed: bool) {
        self.events.push_back(u32::from(code) | (u32::from(pressed) << 31));
    }

    pub fn load(&mut self, offset: u64, size: usize) -> Option<u64> {
        match (offset, size) {
            (0, 4) => Some(self.events.len() as u64),
            (4, 4) => self.events.pop_front().map(u64::from),
            _ => None,
        }
    }
}
