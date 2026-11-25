use crate::guest::GuestAccess;
use crate::riscv::{registers::CustomInstruction, Trap};

/// What a CPU's custom-3 opcode reaches: the RoCC port of the hart.
pub trait Rocc {
    fn custom(&mut self, guest: GuestAccess<'_>, request: CustomInstruction, cycle: u64) -> Result<u64, Trap>;
    /// The custom-0 opcode: illegal unless the hart's RoCC device decodes it.
    fn custom0(&mut self, _guest: GuestAccess<'_>, request: CustomInstruction, _cycle: u64) -> Result<u64, Trap> {
        Err(Trap::illegal(request.instruction))
    }
    /// Frontend acceptance and completion of all submitted accelerator work.
    fn cycles(&self) -> (u64, u64);
    /// The last instruction reached a barrier; the CPU stops stepping.
    fn barrier(&self) -> bool;
}

/// A RoCC port with nothing connected.
pub struct NoRocc;

impl Rocc for NoRocc {
    fn cycles(&self) -> (u64, u64) {
        (0, 0)
    }
    fn custom(&mut self, _guest: GuestAccess<'_>, request: CustomInstruction, _cycle: u64) -> Result<u64, Trap> {
        Err(Trap::illegal(request.instruction))
    }
    fn barrier(&self) -> bool {
        false
    }
}
