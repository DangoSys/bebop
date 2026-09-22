pub const BASE: u64 = 0x0c00_0000;
pub const SIZE: u64 = 0x0400_0000;

const CONTEXT_COUNT: usize = 128;
const ENABLE_BASE: u64 = 0x2000;
const ENABLE_STRIDE: u64 = 0x80;
const CONTEXT_BASE: u64 = 0x20_0000;
const CONTEXT_STRIDE: u64 = 0x1000;

pub struct Plic {
    priority: u32,
    pending: u32,
    enable: [u32; CONTEXT_COUNT],
    threshold: [u32; CONTEXT_COUNT],
}

impl Default for Plic {
    fn default() -> Self {
        Self {
            priority: 0,
            pending: 0,
            enable: [0; CONTEXT_COUNT],
            threshold: [0; CONTEXT_COUNT],
        }
    }
}

impl Plic {
    pub fn load(&self, offset: u64, size: usize) -> Option<u64> {
        if size != 4 {
            return None;
        }
        match offset {
            4 => Some(self.priority.into()),
            0x1000 => Some(self.pending.into()),
            _ if (ENABLE_BASE..CONTEXT_BASE).contains(&offset) => {
                let context = ((offset - ENABLE_BASE) / ENABLE_STRIDE) as usize;
                (context < CONTEXT_COUNT).then(|| self.enable[context].into())
            }
            _ if offset >= CONTEXT_BASE => {
                let context = ((offset - CONTEXT_BASE) / CONTEXT_STRIDE) as usize;
                let register = (offset - CONTEXT_BASE) % CONTEXT_STRIDE;
                if context >= CONTEXT_COUNT {
                    None
                } else if register == 0 {
                    Some(self.threshold[context].into())
                } else if register == 4 {
                    Some(self.claim(context).into())
                } else {
                    None
                }
            }
            _ => Some(0),
        }
    }

    pub fn store(&mut self, offset: u64, size: usize, value: u64) -> bool {
        if size != 4 {
            return false;
        }
        match offset {
            4 => self.priority = value as u32,
            _ if (ENABLE_BASE..CONTEXT_BASE).contains(&offset) => {
                let context = ((offset - ENABLE_BASE) / ENABLE_STRIDE) as usize;
                if context >= CONTEXT_COUNT {
                    return false;
                }
                self.enable[context] = value as u32;
            }
            _ if offset >= CONTEXT_BASE => {
                let context = ((offset - CONTEXT_BASE) / CONTEXT_STRIDE) as usize;
                let register = (offset - CONTEXT_BASE) % CONTEXT_STRIDE;
                if context >= CONTEXT_COUNT {
                    return false;
                }
                if register == 0 {
                    self.threshold[context] = value as u32;
                } else if register == 4 {
                    self.pending &= !(1 << (value as u32 & 31));
                } else {
                    return false;
                }
            }
            _ => return offset < ENABLE_BASE,
        }
        true
    }

    fn claim(&self, context: usize) -> u32 {
        if self.priority > self.threshold[context] && self.pending & self.enable[context] & 2 != 0 {
            1
        } else {
            0
        }
    }
}
