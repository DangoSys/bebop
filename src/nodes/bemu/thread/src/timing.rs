use std::collections::VecDeque;

/// Event times for the frontend, bank dependencies, and execution resources.
/// Advancing a guest clock never iterates over idle hardware cycles.
pub(crate) struct Timing {
    pub(crate) complete: u64,
    pub(crate) accepted: u64,
    writes: Vec<u64>,
    reads: Vec<u64>,
    resources: Vec<u64>,
    memory_reads: u64,
    memory_writes: u64,
    retirement: VecDeque<u64>,
    entries: usize,
}

impl Timing {
    /// Bank ids come from the 10-bit bank fields of an instruction, which also
    /// name the operator and MMIO banks above the virtual banks.
    pub(crate) fn new(balls: usize, entries: usize) -> Self {
        Self {
            complete: 0,
            accepted: 0,
            writes: vec![0; 1 << 10],
            reads: vec![0; 1 << 10],
            resources: vec![0; balls + 4],
            memory_reads: 0,
            memory_writes: 0,
            retirement: VecDeque::with_capacity(entries),
            entries,
        }
    }

    pub(crate) fn reset(&mut self) {
        self.complete = 0;
        self.accepted = 0;
        self.writes.fill(0);
        self.reads.fill(0);
        self.resources.fill(0);
        self.memory_reads = 0;
        self.memory_writes = 0;
        self.retirement.clear();
    }

    pub(crate) fn ready(&self, cycle: u64) -> u64 {
        let accepted = cycle.max(self.accepted + 1);
        if self.retirement.len() == self.entries {
            accepted.max(self.retirement[0])
        } else {
            accepted
        }
    }

    pub(crate) fn issue(
        &mut self,
        cycle: u64,
        funct: u8,
        xs1: u64,
        xs2: u64,
        ball: Option<u32>,
        latency: impl FnOnce(u64) -> u64,
    ) -> (u64, u64) {
        let accepted = self.ready(cycle);
        while self.retirement.front().is_some_and(|&end| end <= accepted) {
            self.retirement.pop_front();
        }
        self.accepted = accepted;
        let resource = match funct {
            16 | 18 | 19 => 0,
            33..=37 | 44 => 1,
            32 => 2,
            1 | 13 | 79 => 3,
            _ => 4 + ball.expect("Ball timing requires an instance") as usize,
        };
        let enable = funct >> 4;
        let bank0 = (xs1 & 1023) as usize;
        let bank1 = ((xs1 >> 10) & 1023) as usize;
        let destination = if funct == 32 {
            bank0
        } else {
            ((xs1 >> 20) & 511) as usize
        };
        let read0 = matches!(enable, 1 | 3 | 4);
        let read1 = enable == 4;
        let write = matches!(enable, 2 | 3 | 4) && funct != 35;
        let update = xs1 >> 63 != 0 || funct == 34 || (funct == 33 && xs2 >> 63 != 0);
        let read_old = xs1 & (1 << 29) != 0 || update;
        let mut start = (accepted + 2).max(self.resources[resource]);
        let memory_read = matches!(funct, 33..=37 | 44);
        let memory_write = matches!(funct, 16 | 18 | 19);
        if memory_read || memory_write {
            start = start.max(self.memory_writes);
        }
        if memory_write {
            start = start.max(self.memory_reads);
        }
        if read0 {
            start = start.max(self.writes[bank0]);
        }
        if read1 {
            start = start.max(self.writes[bank1]);
        }
        if write && (funct == 32 || read_old) {
            start = start.max(self.writes[destination]);
        }
        if write && (funct == 32 || update) {
            start = start.max(self.reads[destination]);
        }
        if funct == 1 || (funct == 32 && xs2 & (1 << 10) == 0) {
            start = start.max(self.complete);
        }
        let duration = latency(start);
        let end = start + duration;
        if memory_read {
            self.memory_reads = self.memory_reads.max(end + 2);
        }
        if memory_write {
            self.memory_writes = end + 2;
        }
        self.resources[resource] = end + 2;
        if read0 {
            self.reads[bank0] = self.reads[bank0].max(end);
        }
        if read1 {
            self.reads[bank1] = self.reads[bank1].max(end);
        }
        if write {
            self.writes[destination] = end + 3;
            // A renamed destination has independent storage from its readers.
            if !update {
                self.reads[destination] = 0;
            }
        }
        self.complete = self.complete.max(end + 2);
        let retired = self.retirement.back().copied().unwrap_or(accepted).max(end) + 1;
        self.retirement.push_back(retired);
        (start, duration)
    }
}

#[cfg(test)]
mod tests {
    use super::Timing;

    #[test]
    fn frontend_readiness_respects_retirement_capacity() {
        let mut time = Timing::new(1, 1);
        let first = time.issue(10, 33, 1 << 20, 0, None, |_| 100).0;
        let ready = time.ready(20);
        assert!(ready > first + 100);
        assert_eq!(time.ready(20), ready);
        time.issue(20, 33, 2 << 20, 0, None, |_| 10);
        assert_eq!(time.accepted, ready);
    }

    #[test]
    fn dependent_ball_waits_while_an_independent_ball_overlaps() {
        let mut time = Timing::new(2, 16);
        let producer = time.issue(10, 33, 1 << 20, 0, None, |_| 100).0;
        let consumer = time.issue(20, 48, 1 | (2 << 20), 0, Some(0), |_| 20).0;
        let independent = time.issue(30, 48, 3 << 20, 0, Some(1), |_| 20).0;
        assert!(consumer >= producer + 100);
        assert!(independent < consumer);
    }

    #[test]
    fn renamed_write_can_overlap_a_reader_but_an_update_cannot() {
        for update in [false, true] {
            let mut time = Timing::new(2, 16);
            let read = time.issue(10, 17, 1, 0, Some(0), |_| 100).0;
            let write = time
                .issue(20, 32 + 4, (1 << 20) | ((update as u64) << 63), 0, Some(1), |_| 10)
                .0;
            assert_eq!(write >= read + 100, update);
        }
    }

    #[test]
    fn ddr_read_waits_for_a_store_even_with_a_fresh_bank_version() {
        for load in [33, 34, 35, 36, 37, 44] {
            for store in [16, 18, 19] {
                let mut time = Timing::new(1, 16);
                let (write, duration) = time.issue(10, store, 1, 0, None, |_| 135);
                let (read, _) = time.issue(11, load, 2 << 20, 0, None, |_| 100);
                assert!(read >= write + duration);
            }
        }
    }

    #[test]
    fn ddr_ordering_does_not_wait_for_an_unrelated_ball() {
        let mut time = Timing::new(1, 16);
        let (ball, latency) = time.issue(1, 48, 100 << 20, 0, Some(0), |_| 10_000);
        let (first, _) = time.issue(10, 33, 1 << 20, 0, None, |_| 100);
        let (second, _) = time.issue(11, 33, 2 << 20, 0, None, |_| 100);
        assert_eq!(second, first + 102);
        let (store, duration) = time.issue(12, 16, 3, 0, None, |_| 135);
        assert!(store >= second + 100);
        assert!(store + duration < ball + latency);
    }

    #[test]
    fn ddr_ordering_resets_between_accelerator_lifetimes() {
        let mut time = Timing::new(1, 16);
        time.issue(10, 16, 1, 0, None, |_| 10_000);
        time.reset();
        let (read, _) = time.issue(20, 33, 2 << 20, 0, None, |_| 100);
        assert_eq!(read, 22);
    }
}
