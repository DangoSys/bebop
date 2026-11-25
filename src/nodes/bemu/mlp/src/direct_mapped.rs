use generic::timing::CacheTiming;

/// A direct-mapped cache of 64-byte lines, tagged by the address it is given (virtual for an L1
/// I$ or D$): a miss refills its line in `miss_cycles`.
pub struct DirectMapped {
    /// The line each slot holds.
    lines: Vec<u64>,
    miss_cycles: u64,
}

impl DirectMapped {
    /// `lines` is a power of two: the slot of a line is its low bits, as in the hardware.
    pub fn new(lines: usize, miss_cycles: u64) -> Self {
        assert!(
            lines.is_power_of_two(),
            "a direct-mapped cache has a power-of-two number of lines"
        );
        Self {
            lines: vec![u64::MAX; lines],
            miss_cycles,
        }
    }
}

impl CacheTiming for DirectMapped {
    fn access(&mut self, address: u64) -> u64 {
        let line = address >> 6;
        let mask = self.lines.len() - 1;
        let slot = &mut self.lines[line as usize & mask];
        if *slot == line {
            return 0;
        }
        *slot = line;
        self.miss_cycles
    }

    fn invalidate(&mut self) {
        self.lines.fill(u64::MAX);
    }
}
