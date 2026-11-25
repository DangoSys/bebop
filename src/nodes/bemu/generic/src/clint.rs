pub const BASE: u64 = 0x0200_0000;
pub const SIZE: u64 = 0x1_0000;
pub const TICK_CYCLES: u64 = 1000;
pub const CPU_SKEW: u64 = 4096;
// Logical target frequency, matching Chipyard's default peripheral clock.
// FPGA and emulator execution speed does not change guest time units.
pub const SOC_CLOCK_HZ: u64 = 500_000_000;
pub const TIMEBASE_HZ: u32 = (SOC_CLOCK_HZ / TICK_CYCLES) as u32;

use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
    Condvar, Mutex, OnceLock,
};

#[repr(align(64))]
struct Clock {
    wake: Condvar,
    lock: Mutex<()>,
    cycles: AtomicU64,
    next_tick: AtomicU64,
    resume_after: AtomicU64,
    finished: AtomicBool,
    waiting: AtomicBool,
    enabled: AtomicU64,
}

pub struct Clint {
    msip: Vec<AtomicU32>,
    progress: Vec<Clock>,
    participants: OnceLock<Vec<usize>>,
    concurrent: OnceLock<bool>,
    ticks: AtomicU64,
    /// Moves whenever the time or a hart's pending interrupts may have changed.
    events: AtomicU64,
    published_cycles: AtomicU64,
    halted: AtomicBool,
    offset: AtomicU64,
    mtimecmp: Vec<AtomicU64>,
    shared_waiters: AtomicUsize,
    shared_lock: Mutex<()>,
    shared_ready: Condvar,
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
                    resume_after: AtomicU64::new(u64::MAX),
                    finished: AtomicBool::new(false),
                    waiting: AtomicBool::new(false),
                    enabled: AtomicU64::new(0),
                })
                .collect(),
            participants: OnceLock::new(),
            concurrent: OnceLock::new(),
            ticks: AtomicU64::new(0),
            events: AtomicU64::new(0),
            published_cycles: AtomicU64::new(0),
            halted: AtomicBool::new(false),
            offset: AtomicU64::new(0),
            mtimecmp: (0..harts).map(|_| AtomicU64::new(u64::MAX)).collect(),
            shared_waiters: AtomicUsize::new(0),
            shared_lock: Mutex::new(()),
            shared_ready: Condvar::new(),
        }
    }

    // Whole-design virtual time advances only when every explicit participant has progressed.
    pub fn coordinate(&self, harts: Vec<usize>, concurrent: bool) {
        assert!(!harts.is_empty());
        assert!(harts.iter().all(|id| *id < self.progress.len()));
        self.participants
            .set(harts)
            .expect("CLINT participants configured twice");
        self.concurrent
            .set(concurrent)
            .expect("CLINT scheduler configured twice");
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
        self.notify_shared();
        true
    }

    // Each progress slot has one writer: its owning hart.
    #[inline]
    pub fn advance_to(&self, hart: usize, cycles: u64) {
        let progress = &self.progress[hart];
        let previous = progress.cycles.load(Ordering::Relaxed);
        if cycles > previous {
            progress.cycles.store(cycles, Ordering::Relaxed);
            let published = self.cycles();
            if cycles >= progress.next_tick.load(Ordering::Relaxed) {
                progress
                    .next_tick
                    .store((cycles / TICK_CYCLES + 1) * TICK_CYCLES, Ordering::Relaxed);
                self.ticks.fetch_max(published / TICK_CYCLES, Ordering::SeqCst);
                self.notify_pending();
            }
            self.notify_progress(published, false);
            self.notify_shared();
        }
    }
    #[inline]
    pub fn published_cycles(&self) -> u64 {
        self.published_cycles.load(Ordering::Acquire)
    }

    pub fn notify_shared(&self) {
        if self.shared_waiters.load(Ordering::Acquire) != 0 {
            let _guard = self.shared_lock.lock().unwrap();
            self.shared_ready.notify_all();
        }
    }

    pub fn wait_shared(&self, ready: impl Fn() -> bool) -> bool {
        self.shared_waiters.fetch_add(1, Ordering::AcqRel);
        let mut guard = self.shared_lock.lock().unwrap();
        while !self.halted.load(Ordering::Acquire) && !ready() {
            guard = self.shared_ready.wait(guard).unwrap();
        }
        self.shared_waiters.fetch_sub(1, Ordering::AcqRel);
        !self.halted.load(Ordering::Acquire)
    }

    // A sleeping owner can be woken by another CPU or a scheduled interrupt.
    pub fn shared_time(&self, hart: usize) -> u64 {
        let owner = &self.progress[hart];
        if self.halted.load(Ordering::Acquire) || owner.finished.load(Ordering::Acquire) {
            return u64::MAX;
        }
        if !owner.waiting.load(Ordering::Acquire) {
            return owner.cycles.load(Ordering::Acquire);
        }
        if !self.concurrent() {
            return u64::MAX;
        }
        self.global_time()
    }

    pub fn concurrent(&self) -> bool {
        *self.concurrent.get().expect("CLINT scheduler configured")
    }

    pub fn global_time(&self) -> u64 {
        let pending = self
            .participants
            .get()
            .expect("CLINT participants configured")
            .iter()
            .any(|&id| {
                let clock = &self.progress[id];
                let enabled = clock.enabled.load(Ordering::Acquire);
                !clock.finished.load(Ordering::Acquire)
                    && (!clock.waiting.load(Ordering::Acquire)
                        || enabled & (1 << 3) != 0 && self.msip[id].load(Ordering::Acquire) != 0
                        || enabled & (1 << 7) != 0 && self.mtimecmp[id].load(Ordering::Acquire) != u64::MAX)
            });
        if pending {
            self.cycles()
        } else {
            u64::MAX
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
        let published = self.published_cycles.load(Ordering::Acquire);
        if candidate <= published {
            published
        } else {
            candidate.max(self.published_cycles.fetch_max(candidate, Ordering::SeqCst))
        }
    }

    pub fn set_waiting(&self, hart: usize, waiting: bool, enabled: u64) {
        let clock = &self.progress[hart];
        clock.enabled.store(enabled, Ordering::Relaxed);
        clock.waiting.store(waiting, Ordering::Release);
        self.notify_progress(self.published_cycles(), true);
        self.notify_shared();
    }
    pub fn finish(&self, hart: usize) {
        self.progress[hart].finished.store(true, Ordering::Release);
        self.ticks.fetch_max(self.cycles() / TICK_CYCLES, Ordering::SeqCst);
        self.notify_pending();
        self.notify_progress(self.published_cycles(), true);
        self.notify_shared();
    }
    pub fn halt(&self) {
        if !self.halted.swap(true, Ordering::AcqRel) {
            self.notify_shared();
            self.notify_progress(self.published_cycles(), true);
            for clock in &self.progress {
                let _guard = clock.lock.lock().unwrap();
                clock.wake.notify_all();
            }
        }
    }

    fn notify_pending(&self) {
        self.events.fetch_add(1, Ordering::Release);
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

    fn notify_progress(&self, cycle: u64, force: bool) {
        // State changes wake waiters without advancing time before an IPI
        // has been posted. Only an actual hart advance publishes new time.
        for clock in &self.progress {
            let deadline = clock.resume_after.load(Ordering::Acquire);
            if deadline != u64::MAX && (force || cycle > deadline) {
                let _guard = clock.lock.lock().unwrap();
                clock.resume_after.store(u64::MAX, Ordering::Release);
                clock.wake.notify_one();
            }
        }
    }

    pub fn wait_for_progress(&self, hart: usize, cycle: u64) {
        let clock = &self.progress[hart];
        loop {
            // Publishing time can also release another throttled hart. Notify
            // outside this hart's lock, which the notifier itself acquires.
            self.notify_progress(self.cycles(), false);
            let mut guard = clock.lock.lock().unwrap();
            clock.resume_after.store(cycle, Ordering::Release);
            if self.halted.load(Ordering::Acquire) || self.published_cycles() > cycle {
                break;
            }
            guard = clock.wake.wait(guard).unwrap();
            drop(guard);
        }
        clock.resume_after.store(u64::MAX, Ordering::Release);
    }

    pub fn wait(&self, hart: usize, enabled: u64) {
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
    pub fn events(&self) -> u64 {
        self.events.load(Ordering::Acquire)
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
    fn shared_events_wait_for_a_running_owner() {
        use std::sync::mpsc;
        use std::time::Duration;
        let clint = Clint::new(1);
        clint.coordinate(vec![0], true);
        clint.advance_to(0, 100);
        std::thread::scope(|scope| {
            let (ready, observed) = mpsc::channel();
            let clock = &clint;
            scope.spawn(move || {
                assert!(clock.wait_shared(|| clock.shared_time(0) >= 200));
                ready.send(()).unwrap();
            });
            assert!(observed.recv_timeout(Duration::from_millis(10)).is_err());
            clint.advance_to(0, 200);
            observed.recv_timeout(Duration::from_secs(1)).unwrap();
        });
    }

    #[test]
    fn sleeping_owner_tracks_other_running_cpus() {
        let clint = Clint::new(2);
        clint.coordinate(vec![0, 1], true);
        clint.advance_to(0, 100);
        clint.advance_to(1, 120);
        clint.set_waiting(0, true, 1 << 3);
        assert_eq!(clint.shared_time(0), 120);
        clint.advance_to(1, 200);
        assert_eq!(clint.shared_time(0), 200);
        clint.set_waiting(1, true, 0);
        assert_eq!(clint.shared_time(0), u64::MAX);
    }

    #[test]
    fn a_waiter_publishing_progress_wakes_other_throttled_harts() {
        use std::sync::{atomic::Ordering, mpsc};
        use std::time::Duration;
        let clint = Clint::new(3);
        clint.coordinate(vec![0, 1, 2], true);
        clint.advance_to(0, 8_000);
        clint.advance_to(1, 8_000);
        std::thread::scope(|scope| {
            let (sent, received) = mpsc::channel();
            let clock = &clint;
            scope.spawn(move || {
                clock.wait_for_progress(0, 4_000);
                sent.send(()).unwrap();
            });
            while clint.progress[0].resume_after.load(Ordering::Acquire) != 4_000 {
                std::thread::yield_now();
            }
            // Isolate publication by the waiter from the state-change notifier.
            let guard = clint.progress[0].lock.lock().unwrap();
            clint.progress[2].waiting.store(true, Ordering::Release);
            drop(guard);
            clint.wait_for_progress(1, 4_000);
            let resumed = received.recv_timeout(Duration::from_secs(1)).is_ok();
            clint.halt();
            assert!(resumed, "published progress left another hart asleep");
        });
    }

    #[test]
    fn progress_wakes_only_harts_past_their_deadline() {
        use std::sync::atomic::Ordering;
        let clint = Clint::new(3);
        clint.coordinate(vec![0, 1, 2], true);
        clint.advance_to(0, 8_000);
        clint.advance_to(1, 8_000);
        std::thread::scope(|scope| {
            let first = scope.spawn(|| clint.wait_for_progress(0, 1_000));
            let second = scope.spawn(|| clint.wait_for_progress(1, 4_000));
            while clint.progress[0].resume_after.load(Ordering::Acquire) != 1_000
                || clint.progress[1].resume_after.load(Ordering::Acquire) != 4_000
            {
                std::thread::yield_now();
            }
            clint.advance_to(2, 2_000);
            first.join().unwrap();
            assert_eq!(clint.progress[1].resume_after.load(Ordering::Acquire), 4_000);
            clint.advance_to(2, 5_000);
            second.join().unwrap();
        });
    }

    #[test]
    fn throttled_harts_resume_on_progress_finish_and_shutdown() {
        let clint = Clint::new(2);
        clint.coordinate(vec![0, 1], true);
        clint.advance_to(0, 8_000);
        std::thread::scope(|scope| {
            let waiter = scope.spawn(|| clint.wait_for_progress(0, 1_000));
            clint.advance_to(1, 2_000);
            waiter.join().unwrap();
            let waiter = scope.spawn(|| clint.wait_for_progress(0, 4_000));
            clint.finish(1);
            waiter.join().unwrap();
            let waiter = scope.spawn(|| clint.wait_for_progress(0, 16_000));
            clint.halt();
            waiter.join().unwrap();
        });
    }

    #[test]
    fn a_pending_software_interrupt_prevents_a_timer_jump() {
        let clint = Clint::new(1);
        clint.coordinate(vec![0], true);
        clint.advance_to(0, 1_000);
        assert!(clint.store(0x4000, 8, 100));
        clint.set_waiting(0, true, (1 << 3) | (1 << 7));
        assert!(clint.store(0, 4, 1));
        assert_eq!(clint.cycles(), 1_000);
    }

    #[test]
    fn parked_harts_wake_for_software_interrupts_and_shutdown() {
        let clint = Clint::new(2);
        clint.coordinate(vec![0, 1], true);
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
        clint.coordinate(vec![0, 1], true);
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
        clint.coordinate(vec![0, 2], true);
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
