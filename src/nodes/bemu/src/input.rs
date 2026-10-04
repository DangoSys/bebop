use bebop_clint::Clint;
use rvsim::input::Environment;
use std::cell::Cell;

pub(crate) struct Inputs<'a> {
    pub(crate) cycles: u64,
    pub(crate) hart: usize,
    pub(crate) clint: &'a Clint,
    pub(crate) snapshot: Cell<Option<(u64, u64)>>,
}

impl Inputs<'_> {
    fn sample(&self) -> (u64, u64) {
        if let Some(snapshot) = self.snapshot.get() {
            return snapshot;
        }
        let snapshot = self.clint.sample(self.hart);
        self.snapshot.set(Some(snapshot));
        snapshot
    }
}

impl Environment for Inputs<'_> {
    fn cycles(&self) -> u64 {
        self.cycles
    }
    fn time(&self) -> u64 {
        self.sample().0
    }
    fn interrupts(&self) -> u64 {
        self.sample().1
    }
}
