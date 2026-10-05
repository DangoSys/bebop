pub const BASE: u64 = 0x0200_0000;
pub const SIZE: u64 = 0x1_0000;

use std::sync::{OnceLock, atomic::{AtomicU32, AtomicU64, Ordering}};

#[repr(align(64))]
struct Clock(AtomicU64);

pub struct Clint {
    msip: Vec<AtomicU32>,
    progress: Vec<Clock>,
    participants: OnceLock<Vec<usize>>,
    offset: AtomicU64,
    mtimecmp: Vec<AtomicU64>,
}

impl Clint {
    pub fn new(harts: usize) -> Self {
        Self {
            msip: (0..harts).map(|_| AtomicU32::new(0)).collect(),
            progress: (0..harts).map(|_| Clock(AtomicU64::new(0))).collect(),
            participants: OnceLock::new(),
            offset: AtomicU64::new(0),
            mtimecmp: (0..harts).map(|_| AtomicU64::new(u64::MAX)).collect(),
        }
    }

    // Whole-chip virtual time advances only when every explicit participant has progressed.
    pub fn coordinate(&self, harts: Vec<usize>) {
        assert!(!harts.is_empty());
        assert!(harts.iter().all(|id| *id < self.progress.len()));
        self.participants.set(harts).expect("CLINT participants configured twice");
    }

    pub fn load(&self, offset: u64, size: usize) -> Option<u64> {
        match (offset, size) {
            (0xbff8, 8) => Some(self.time()),
            (0xbff8, 4) => Some(self.time() & u32::MAX as u64),
            (0xbffc, 4) => Some(self.time() >> 32),
            (offset, 4) if offset < 0x4000 && offset % 4 == 0 => self
                .msip
                .get(offset as usize / 4)
                .map(|v| u64::from(v.load(Ordering::Relaxed))),
            (offset, 4 | 8) if (0x4000..0xbff8).contains(&offset) && offset % size as u64 == 0 => {
                let value = self
                    .mtimecmp
                    .get((offset as usize - 0x4000) / 8)?
                    .load(Ordering::Relaxed);
                Some(if size == 8 {
                    value
                } else {
                    (value >> ((offset & 4) * 8)) & u32::MAX as u64
                })
            }
            _ => None,
        }
    }

    pub fn store(&self, offset: u64, size: usize, value: u64) -> bool {
        match (offset, size) {
            (0xbff8, 8) => self.offset.store(value.wrapping_sub(self.cycles()), Ordering::Relaxed),
            (0xbff8 | 0xbffc, 4) => {
                let shift = (offset & 4) * 8;
                let cycles = self.cycles();
                self.offset
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                        let time = cycles.wrapping_add(old);
                        let time = (time & !((u32::MAX as u64) << shift)) | ((value as u32 as u64) << shift);
                        Some(time.wrapping_sub(cycles))
                    })
                    .unwrap();
            }
            (offset, 4) if offset < 0x4000 && offset % 4 == 0 => {
                let Some(msip) = self.msip.get(offset as usize / 4) else {
                    return false;
                };
                msip.store(value as u32 & 1, Ordering::Relaxed);
            }
            (offset, 4 | 8) if (0x4000..0xbff8).contains(&offset) && offset % size as u64 == 0 => {
                let Some(compare) = self.mtimecmp.get((offset as usize - 0x4000) / 8) else {
                    return false;
                };
                if size == 8 {
                    compare.store(value, Ordering::Relaxed);
                } else {
                    let shift = (offset & 4) * 8;
                    compare
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
                            Some((old & !((u32::MAX as u64) << shift)) | ((value as u32 as u64) << shift))
                        })
                        .unwrap();
                }
            }
            _ => return false,
        }
        true
    }

    // Each progress slot has one writer: its owning hart.
    #[inline]
    pub fn advance_to(&self, hart: usize, cycles: u64) {
        let progress = &self.progress[hart].0;
        if cycles > progress.load(Ordering::Relaxed) {
            progress.store(cycles, Ordering::Relaxed);
        }
    }
    #[inline]
    pub fn cycles(&self) -> u64 {
        if let Some(harts) = self.participants.get() {
            harts.iter().map(|id| self.progress[*id].0.load(Ordering::Relaxed)).min().unwrap()
        } else {
            self.progress.iter().map(|clock| clock.0.load(Ordering::Relaxed)).max().unwrap()
        }
    }
    #[inline]
    pub fn sample(&self, hart: usize) -> (u64, u64) {
        let time = self.time();
        let interrupts = (u64::from(self.msip[hart].load(Ordering::Relaxed) != 0) << 3)
            | (u64::from(time >= self.mtimecmp[hart].load(Ordering::Relaxed)) << 7);
        (time, interrupts)
    }
    #[inline]
    pub fn time(&self) -> u64 {
        self.cycles().wrapping_add(self.offset.load(Ordering::Relaxed))
    }
}
