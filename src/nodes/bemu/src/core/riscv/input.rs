pub trait Environment {
    fn cycles(&self) -> u64;
    fn time(&self) -> u64;
    fn interrupts(&self) -> u64;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Inputs {
    pub time: u64,
    pub cycles: u64,
    pub interrupts: u64,
}

impl Environment for Inputs {
    fn cycles(&self) -> u64 {
        self.cycles
    }
    fn time(&self) -> u64 {
        self.time
    }
    fn interrupts(&self) -> u64 {
        self.interrupts
    }
}
