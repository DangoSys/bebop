pub const BASE: u64 = 0x1100_0000;
pub const SIZE: u64 = 0x20_0000;
pub const FRAMEBUFFER: u64 = 0x1000;
pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 480;

pub struct Vga {
    pixels: Vec<u8>, // Little-endian XRGB8888.
    frame: Option<Vec<u8>>,
    sequence: u32,
}

impl Default for Vga {
    fn default() -> Self {
        Self {
            pixels: vec![0; WIDTH * HEIGHT * 4],
            frame: None,
            sequence: 0,
        }
    }
}

impl Vga {
    pub fn take_frame(&mut self) -> Option<Vec<u8>> {
        self.frame.take()
    }

    pub fn load(&self, offset: u64, size: usize) -> Option<u64> {
        match (offset, size) {
            (0, 4) => Some(WIDTH as u64),
            (4, 4) => Some(HEIGHT as u64),
            (8, 4) => Some((WIDTH * 4) as u64),
            (12, 4) => Some(self.sequence.into()),
            (offset, 1 | 2 | 4) if offset >= FRAMEBUFFER && offset % size as u64 == 0 => {
                let start = (offset - FRAMEBUFFER) as usize;
                let mut value = [0; 8];
                value[..size].copy_from_slice(self.pixels.get(start..start + size)?);
                Some(u64::from_le_bytes(value))
            }
            _ => None,
        }
    }

    pub fn store(&mut self, offset: u64, size: usize, value: u64) -> bool {
        match (offset, size) {
            (12, 4) if value == 1 => {
                self.frame = Some(self.pixels.clone());
                self.sequence = self.sequence.wrapping_add(1);
            }
            (offset, 1 | 2 | 4) if offset >= FRAMEBUFFER && offset % size as u64 == 0 => {
                let start = (offset - FRAMEBUFFER) as usize;
                let Some(bytes) = self.pixels.get_mut(start..start + size) else {
                    return false;
                };
                bytes.copy_from_slice(&value.to_le_bytes()[..size]);
            }
            _ => return false,
        }
        true
    }
}
