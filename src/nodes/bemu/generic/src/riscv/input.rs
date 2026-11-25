pub trait Environment {
    fn time(&self) -> u64;
    fn interrupts(&self) -> u64;
    /// A count that moves whenever the time or the interrupts may have changed.
    fn events(&self) -> u64;
    /// Drain attached accelerator work before a CPU data access or management command.
    fn memory_wait_cycles(&self) -> u64;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Inputs {
    pub time: u64,
    pub interrupts: u64,
}

impl Environment for Inputs {
    fn time(&self) -> u64 {
        self.time
    }
    fn interrupts(&self) -> u64 {
        self.interrupts
    }
    fn events(&self) -> u64 {
        0
    }
    fn memory_wait_cycles(&self) -> u64 {
        0
    }
}
