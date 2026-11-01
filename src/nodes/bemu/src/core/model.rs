use super::{
    riscv::{hart::CustomInstruction, Trap},
    Cpu, GuestAccess,
};
use bebop_bemu_profile::BemuProfileReport;
use snafu::Whatever;
use std::time::Duration;

/// What a CPU's custom-3 opcode reaches: the RoCC port of the core.
pub trait Rocc {
    fn custom(&mut self, guest: GuestAccess<'_>, request: CustomInstruction, cycle: u64) -> Result<u64, Trap>;
    /// The last instruction reached a barrier; the CPU stops stepping.
    fn barrier(&self) -> bool;
}

/// A RoCC port with nothing connected.
pub struct NoRocc;

impl Rocc for NoRocc {
    fn custom(&mut self, _guest: GuestAccess<'_>, request: CustomInstruction, _cycle: u64) -> Result<u64, Trap> {
        Err(Trap::illegal(request.instruction))
    }
    fn barrier(&self) -> bool {
        false
    }
}

/// A core of the device as the simulator runs it: what a CoreFactory builds.
pub trait CoreModel: Send {
    fn cpu(&self) -> &Cpu;
    fn cpu_mut(&mut self) -> &mut Cpu;
    /// Steps the core by up to `count` instructions.
    fn step(&mut self, count: u64) -> Result<(), Whatever>;
    fn take_barrier(&mut self) -> bool {
        false
    }
    fn profile_report(&self, total: Duration) -> Option<BemuProfileReport> {
        self.cpu().profile_report(total)
    }
}
