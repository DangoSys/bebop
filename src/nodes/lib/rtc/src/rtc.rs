pub const BASE: u64 = 0x1000_4000;
pub const SIZE: u64 = 0x1000;

pub struct Rtc {
    ns: u64,
    high: u32,
}

impl Rtc {
    pub fn new(unix_ns: u64) -> Self {
        Self {
            ns: unix_ns,
            high: (unix_ns >> 32) as u32,
        }
    }

    pub fn tick(&mut self, ns: u64) {
        self.ns = self.ns.checked_add(ns).expect("RTC time overflow");
    }

    pub fn load(&mut self, offset: u64, size: usize) -> Option<u64> {
        match (offset, size) {
            (0, 8) => Some(self.ns),
            (0, 4) => {
                self.high = (self.ns >> 32) as u32;
                Some(self.ns as u32 as u64)
            }
            (4, 4) => Some(self.high.into()), // Latched by the low-word read.
            _ => None,
        }
    }

    pub fn store(&mut self, offset: u64, size: usize, value: u64) -> bool {
        match (offset, size) {
            (0, 8) => self.ns = value,
            (0, 4) => self.ns = (self.ns & !u64::from(u32::MAX)) | value as u32 as u64,
            (4, 4) => self.ns = (self.ns & u64::from(u32::MAX)) | ((value as u32 as u64) << 32),
            _ => return false,
        }
        true
    }
}
