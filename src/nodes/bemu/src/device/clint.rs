pub const BASE: u64 = 0x0200_0000;
pub const SIZE: u64 = 0x1_0000;
pub const TICK_CYCLES: u64 = 1000;
// Logical target frequency, matching Chipyard's default peripheral clock.
// FPGA and emulator execution speed does not change guest time units.
pub const SOC_CLOCK_HZ: u64 = 500_000_000;
pub const TIMEBASE_HZ: u32 = (SOC_CLOCK_HZ / TICK_CYCLES) as u32;

use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    Condvar, Mutex, OnceLock,
};

#[repr(align(64))]
struct Clock {
    wake: Condvar,
    lock: Mutex<()>,
    cycles: AtomicU64,
    next_tick: AtomicU64,
    finished: AtomicBool,
    waiting: AtomicBool,
    enabled: AtomicU64,
}

pub struct Clint {
    msip: Vec<AtomicU32>,
    progress: Vec<Clock>,
    participants: OnceLock<Vec<usize>>,
    ticks: AtomicU64,
    published_cycles: AtomicU64,
    halted: AtomicBool,
    progress_lock: Mutex<usize>,
    progress_wake: Condvar,
    offset: AtomicU64,
    mtimecmp: Vec<AtomicU64>,
}

impl Clint {
    pub fn new(harts: usize) -> Self {
        Self {
            msip: (0..harts).map(|_| AtomicU32::new(0)).collect(),
            progress: (0..harts)
                .map(|_| Clock {
                    wake: Condvar::new(),
                    lock: Mutex::new(()),
                    cycles: AtomicU64::new(0),
                    next_tick: AtomicU64::new(TICK_CYCLES),
                    finished: AtomicBool::new(false),
                    waiting: AtomicBool::new(false),
                    enabled: AtomicU64::new(0),
                })
                .collect(),
            participants: OnceLock::new(),
            ticks: AtomicU64::new(0),
            published_cycles: AtomicU64::new(0),
            halted: AtomicBool::new(false),
            progress_lock: Mutex::new(0),
            progress_wake: Condvar::new(),
            offset: AtomicU64::new(0),
            mtimecmp: (0..harts).map(|_| AtomicU64::new(u64::MAX)).collect(),
        }
    }

    // Whole-design virtual time advances only when every explicit participant has progressed.
    pub fn coordinate(&self, harts: Vec<usize>) {
        assert!(!harts.is_empty());
        assert!(harts.iter().all(|id| *id < self.progress.len()));
        self.participants
            .set(harts)
            .expect("CLINT participants configured twice");
        self.ticks.store(self.cycles() / TICK_CYCLES, Ordering::SeqCst);
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
            (0xbff8, 8) => self
                .offset
                .store(value.wrapping_sub(self.cycles() / TICK_CYCLES), Ordering::Relaxed),
            (0xbff8 | 0xbffc, 4) => {
                let shift = (offset & 4) * 8;
                let cycles = self.cycles() / TICK_CYCLES;
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
        self.notify_pending();
        true
    }

    // Each progress slot has one writer: its owning hart.
    #[inline]
    pub fn advance_to(&self, hart: usize, cycles: u64) {
        let progress = &self.progress[hart];
        let previous = progress.cycles.load(Ordering::Relaxed);
        if cycles > previous {
            progress.cycles.store(cycles, Ordering::Relaxed);
            if cycles >= progress.next_tick.load(Ordering::Relaxed) {
                progress
                    .next_tick
                    .store((cycles / TICK_CYCLES + 1) * TICK_CYCLES, Ordering::Relaxed);
                self.ticks.fetch_max(self.cycles() / TICK_CYCLES, Ordering::SeqCst);
                self.notify_pending();
            }
            self.notify_progress();
        }
    }
    #[inline]
    pub fn cycles(&self) -> u64 {
        let candidate = if self.halted.load(Ordering::Relaxed) {
            self.progress
                .iter()
                .map(|p| p.cycles.load(Ordering::Relaxed))
                .max()
                .unwrap()
        } else if let Some(harts) = self.participants.get() {
            let mut active = None;
            let mut wake = u64::MAX;
            for &hart in harts {
                let clock = &self.progress[hart];
                if clock.finished.load(Ordering::Acquire) {
                    continue;
                }
                if clock.waiting.load(Ordering::Acquire) {
                    let enabled = clock.enabled.load(Ordering::Relaxed);
                    if enabled & (1 << 3) != 0 && self.msip[hart].load(Ordering::Relaxed) != 0 {
                        wake = wake.min(self.published_cycles.load(Ordering::Relaxed));
                    }
                    if enabled & (1 << 7) != 0 {
                        let compare = self.mtimecmp[hart].load(Ordering::Relaxed);
                        let deadline = if compare <= self.time() {
                            self.published_cycles.load(Ordering::Relaxed)
                        } else {
                            compare
                                .wrapping_sub(self.offset.load(Ordering::Relaxed))
                                .saturating_mul(TICK_CYCLES)
                        };
                        wake = wake.min(deadline);
                    }
                } else {
                    let cycle = clock.cycles.load(Ordering::Relaxed);
                    active = Some(active.map_or(cycle, |previous: u64| previous.min(cycle)));
                }
            }
            match active {
                Some(cycle) => cycle.min(wake),
                None if wake != u64::MAX => wake,
                None => self
                    .progress
                    .iter()
                    .map(|p| p.cycles.load(Ordering::Relaxed))
                    .max()
                    .unwrap(),
            }
        } else {
            self.progress
                .iter()
                .map(|p| p.cycles.load(Ordering::Relaxed))
                .max()
                .unwrap()
        };
        candidate.max(self.published_cycles.fetch_max(candidate, Ordering::SeqCst))
    }

    pub(crate) fn set_waiting(&self, hart: usize, waiting: bool, enabled: u64) {
        let clock = &self.progress[hart];
        clock.enabled.store(enabled, Ordering::Relaxed);
        clock.waiting.store(waiting, Ordering::Release);
        self.notify_progress();
    }
    pub(crate) fn finish(&self, hart: usize) {
        self.progress[hart].finished.store(true, Ordering::Release);
        self.ticks.fetch_max(self.cycles() / TICK_CYCLES, Ordering::SeqCst);
        self.notify_pending();
        self.notify_progress();
    }
    pub(crate) fn halt(&self) {
        if !self.halted.swap(true, Ordering::AcqRel) {
            self.notify_progress();
            for clock in &self.progress {
                let _guard = clock.lock.lock().unwrap();
                clock.wake.notify_all();
            }
        }
    }

    fn notify_pending(&self) {
        for (hart, clock) in self.progress.iter().enumerate() {
            if clock.waiting.load(Ordering::Acquire)
                && ((clock.enabled.load(Ordering::Relaxed) & (1 << 3) != 0
                    && self.msip[hart].load(Ordering::Relaxed) != 0)
                    || (clock.enabled.load(Ordering::Relaxed) & (1 << 7) != 0
                        && self.time() >= self.mtimecmp[hart].load(Ordering::Relaxed)))
            {
                let _guard = clock.lock.lock().unwrap();
                clock.wake.notify_one();
            }
        }
    }

    fn notify_progress(&self) {
        let waiters = self.progress_lock.lock().unwrap();
        if *waiters != 0 {
            self.progress_wake.notify_all();
        }
    }

    pub(crate) fn wait_for_progress(&self, cycle: u64) {
        let mut guard = self.progress_lock.lock().unwrap();
        *guard += 1;
        while !self.halted.load(Ordering::Acquire) && self.cycles() <= cycle {
            guard = self.progress_wake.wait(guard).unwrap();
        }
        *guard -= 1;
    }

    pub(crate) fn wait(&self, hart: usize, enabled: u64) {
        let clock = &self.progress[hart];
        let mut guard = clock.lock.lock().unwrap();
        while !self.halted.load(Ordering::Acquire) && self.sample(hart).1 & enabled == 0 {
            guard = clock.wake.wait(guard).unwrap();
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
        self.ticks
            .load(Ordering::SeqCst)
            .wrapping_add(self.offset.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::Clint;

    #[test]
    fn throttled_harts_resume_on_progress_finish_and_shutdown() {
        let clint = Clint::new(2);
        clint.coordinate(vec![0, 1]);
        clint.advance_to(0, 8_000);
        std::thread::scope(|scope| {
            let waiter = scope.spawn(|| clint.wait_for_progress(1_000));
            clint.advance_to(1, 2_000);
            waiter.join().unwrap();
            let waiter = scope.spawn(|| clint.wait_for_progress(4_000));
            clint.finish(1);
            waiter.join().unwrap();
            let waiter = scope.spawn(|| clint.wait_for_progress(16_000));
            clint.halt();
            waiter.join().unwrap();
        });
    }

    #[test]
    fn a_pending_software_interrupt_prevents_a_timer_jump() {
        let clint = Clint::new(1);
        clint.coordinate(vec![0]);
        clint.advance_to(0, 1_000);
        assert!(clint.store(0x4000, 8, 100));
        clint.set_waiting(0, true, (1 << 3) | (1 << 7));
        assert!(clint.store(0, 4, 1));
        assert_eq!(clint.cycles(), 1_000);
    }

    #[test]
    fn parked_harts_wake_for_software_interrupts_and_shutdown() {
        let clint = Clint::new(2);
        clint.coordinate(vec![0, 1]);
        clint.set_waiting(0, true, 1 << 3);
        clint.set_waiting(1, true, 0);
        std::thread::scope(|scope| {
            let interrupt = scope.spawn(|| clint.wait(0, 1 << 3));
            assert!(clint.store(0, 4, 1));
            interrupt.join().unwrap();
            let shutdown = scope.spawn(|| clint.wait(1, 0));
            clint.halt();
            shutdown.join().unwrap();
        });
    }

    #[test]
    fn idle_harts_do_not_throttle_work_and_wake_at_the_timer_deadline() {
        let clint = Clint::new(2);
        clint.coordinate(vec![0, 1]);
        clint.advance_to(0, 1_000);
        clint.advance_to(1, 1_000);
        clint.set_waiting(1, true, 0);
        clint.advance_to(0, 10_000);
        assert_eq!(clint.cycles(), 10_000);
        assert!(clint.store(0x4008, 8, 32));
        clint.set_waiting(1, true, 1 << 7);
        clint.set_waiting(0, true, 0);
        assert_eq!(clint.cycles(), 32_000);
        clint.advance_to(1, 32_000);
        assert_eq!(clint.sample(1).1 & (1 << 7), 1 << 7);
        clint.set_waiting(1, false, 1 << 7);
        assert_eq!(clint.cycles(), 32_000);
    }

    #[test]
    fn a_finished_hart_does_not_stop_the_remaining_harts_clock() {
        let clint = Clint::new(3);
        clint.coordinate(vec![0, 2]);
        clint.advance_to(0, 10_000);
        clint.advance_to(2, 2_000);
        assert_eq!(clint.cycles(), 2_000);
        clint.finish(2);
        assert_eq!(clint.cycles(), 10_000);
        assert_eq!(clint.time(), 10);
        clint.advance_to(0, 13_000);
        clint.finish(0);
        assert_eq!(clint.cycles(), 13_000);
        assert_eq!(clint.time(), 13);
    }
}
