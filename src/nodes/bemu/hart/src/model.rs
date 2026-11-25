use super::Cpu;
use bebop_bemu_profile::BemuProfileReport;
use snafu::Whatever;
use std::time::Duration;

/// A hart of the device as the simulator runs it: what a HartFactory builds.
pub trait HartModel: Send {
    fn cpu(&self) -> &Cpu;
    fn cpu_mut(&mut self) -> &mut Cpu;
    /// Steps the hart by up to `count` instructions.
    fn step(&mut self, count: u64) -> Result<(), Whatever>;
    fn take_barrier(&mut self) -> bool {
        false
    }
    fn profile_report(&self, total: Duration) -> Option<BemuProfileReport> {
        self.cpu().profile_report(total)
    }
}
