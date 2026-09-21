pub const BASE: u64 = 0x0200_0000;
pub const SIZE: u64 = 0x1_0000;

const MSIP: u64 = 0x0000;
const MTIMECMP: u64 = 0x4000;
const MTIMECMP_HI: u64 = 0x4004;
const MTIME: u64 = 0xbff8;
const MTIME_HI: u64 = 0xbffc;

#[derive(Default)]
pub struct Clint {
    msip: u32,
    mtime: u64,
    mtimecmp: u64,
}

impl Clint {
    pub fn new() -> Self {
        Self {
            mtimecmp: u64::MAX,
            ..Self::default()
        }
    }

    pub fn load(&self, offset: u64, size: usize) -> Option<u64> {
        match (offset, size) {
            (MSIP, 4) => Some(self.msip.into()),
            (MTIMECMP, 8) => Some(self.mtimecmp),
            (MTIMECMP, 4) => Some(self.mtimecmp & u32::MAX as u64),
            (MTIMECMP_HI, 4) => Some(self.mtimecmp >> 32),
            (MTIME, 8) => Some(self.mtime),
            (MTIME, 4) => Some(self.mtime & u32::MAX as u64),
            (MTIME_HI, 4) => Some(self.mtime >> 32),
            _ => None,
        }
    }

    pub fn store(&mut self, offset: u64, size: usize, value: u64) -> bool {
        match (offset, size) {
            (MSIP, 4) => self.msip = value as u32 & 1,
            (MTIMECMP, 8) => self.mtimecmp = value,
            (MTIMECMP, 4) => self.mtimecmp = (self.mtimecmp & !u32::MAX as u64) | value as u32 as u64,
            (MTIMECMP_HI, 4) => self.mtimecmp = (self.mtimecmp & u32::MAX as u64) | ((value as u32 as u64) << 32),
            (MTIME, 8) => self.mtime = value,
            (MTIME, 4) => self.mtime = (self.mtime & !u32::MAX as u64) | value as u32 as u64,
            (MTIME_HI, 4) => self.mtime = (self.mtime & u32::MAX as u64) | ((value as u32 as u64) << 32),
            _ => return false,
        }
        true
    }

    pub fn tick(&mut self, cycles: u64) -> u32 {
        self.mtime = self.mtime.wrapping_add(cycles);
        u32::from(self.msip != 0) | (u32::from(self.mtime >= self.mtimecmp) << 1)
    }

    pub fn time(&self) -> u64 {
        self.mtime
    }
}
