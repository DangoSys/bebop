pub const BASE: u64 = 0x0200_0000;
pub const SIZE: u64 = 0x1_0000;

pub struct Clint {
    msip: Vec<u32>,
    mtime: u64,
    mtimecmp: Vec<u64>,
}

impl Clint {
    pub fn new(harts: usize) -> Self {
        Self {
            msip: vec![0; harts],
            mtime: 0,
            mtimecmp: vec![u64::MAX; harts],
        }
    }

    pub fn load(&self, offset: u64, size: usize) -> Option<u64> {
        match (offset, size) {
            (0xbff8, 8) => Some(self.mtime),
            (0xbff8, 4) => Some(self.mtime & u32::MAX as u64),
            (0xbffc, 4) => Some(self.mtime >> 32),
            (offset, 4) if offset < 0x4000 && offset % 4 == 0 => {
                self.msip.get(offset as usize / 4).map(|v| u64::from(*v))
            }
            (offset, 4 | 8) if (0x4000..0xbff8).contains(&offset) && offset % size as u64 == 0 => {
                let value = *self.mtimecmp.get((offset as usize - 0x4000) / 8)?;
                Some(if size == 8 {
                    value
                } else {
                    (value >> ((offset & 4) * 8)) & u32::MAX as u64
                })
            }
            _ => None,
        }
    }

    pub fn store(&mut self, offset: u64, size: usize, value: u64) -> bool {
        match (offset, size) {
            (0xbff8, 8) => self.mtime = value,
            (0xbff8, 4) => self.mtime = (self.mtime & !(u32::MAX as u64)) | value as u32 as u64,
            (0xbffc, 4) => self.mtime = (self.mtime & u32::MAX as u64) | ((value as u32 as u64) << 32),
            (offset, 4) if offset < 0x4000 && offset % 4 == 0 => {
                let Some(msip) = self.msip.get_mut(offset as usize / 4) else {
                    return false;
                };
                *msip = value as u32 & 1;
            }
            (offset, 4 | 8) if (0x4000..0xbff8).contains(&offset) && offset % size as u64 == 0 => {
                let Some(compare) = self.mtimecmp.get_mut((offset as usize - 0x4000) / 8) else {
                    return false;
                };
                if size == 8 {
                    *compare = value;
                } else {
                    let shift = (offset & 4) * 8;
                    *compare = (*compare & !((u32::MAX as u64) << shift)) | ((value as u32 as u64) << shift);
                }
            }
            _ => return false,
        }
        true
    }

    pub fn tick(&mut self, ticks: u64) {
        self.mtime = self.mtime.wrapping_add(ticks);
    }
    pub fn pending(&self, hart: usize) -> u64 {
        (u64::from(self.msip[hart] != 0) << 3) | (u64::from(self.mtime >= self.mtimecmp[hart]) << 7)
    }
    pub fn time(&self) -> u64 {
        self.mtime
    }
}
