use generic::clint::Clint;
use generic::riscv::input::Environment;
use std::cell::Cell;

pub(crate) struct Inputs<'a> {
    pub(crate) hart: usize,
    pub(crate) clint: &'a Clint,
    /// The cycle of the instruction the hart is at.
    pub(crate) clock: &'a Cell<u64>,
    pub(crate) memory_ready: u64,
}

impl Environment for Inputs<'_> {
    #[inline]
    fn time(&self) -> u64 {
        self.clint.sample(self.hart).0
    }
    #[inline]
    fn interrupts(&self) -> u64 {
        self.clint.sample(self.hart).1
    }
    #[inline]
    fn events(&self) -> u64 {
        self.clint.events()
    }
    #[inline]
    fn memory_wait_cycles(&self) -> u64 {
        let cycles = self.memory_ready.saturating_sub(self.clock.get());
        if cycles != 0 {
            self.clint.advance_to(self.hart, self.memory_ready);
            self.clock.set(self.memory_ready);
        }
        cycles
    }
}
