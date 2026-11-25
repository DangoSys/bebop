use super::{
    bus::{Bus, BusError, HartBus, Width},
    input::Environment,
    mmu::Mmu,
    registers::{Registers, Step},
};
use std::cell::Cell;

struct Clock<'a> {
    now: &'a Cell<u64>,
    ready: u64,
    waited: &'a Cell<u64>,
}

impl Environment for Clock<'_> {
    fn time(&self) -> u64 {
        self.now.get()
    }
    fn interrupts(&self) -> u64 {
        0
    }
    fn events(&self) -> u64 {
        0
    }
    fn memory_wait_cycles(&self) -> u64 {
        let delay = self.ready.saturating_sub(self.now.get());
        self.now.set(self.now.get() + delay);
        self.waited.set(delay);
        delay
    }
}

struct Memory<'a> {
    instruction: u32,
    now: &'a Cell<u64>,
    expected_access_cycle: u64,
    value: u64,
}

impl Bus for Memory<'_> {
    fn fetch(&mut self, address: u64, width: Width) -> Result<u64, BusError> {
        assert_eq!(self.now.get(), 13, "instruction fetch must not drain the accelerator");
        assert_eq!(width, Width::Half);
        Ok((self.instruction >> (address * 8)) as u64 & 0xffff)
    }
    fn read(&mut self, address: u64, _: Width) -> Result<u64, BusError> {
        assert_eq!(address, 256);
        assert_eq!(self.now.get(), self.expected_access_cycle);
        Ok(self.value)
    }
    fn write(&mut self, address: u64, _: Width, value: u64) -> Result<(), BusError> {
        assert_eq!(address, 256);
        assert_eq!(self.now.get(), self.expected_access_cycle);
        self.value = value;
        Ok(())
    }
    fn peripheral(&mut self) -> bool {
        false
    }
    fn compare_exchange(&mut self, address: u64, width: Width, expected: u64, value: u64) -> Result<u64, BusError> {
        let old = self.read(address, width)?;
        if old == expected {
            self.write(address, width, value)?;
        }
        Ok(old)
    }
}

impl HartBus for Memory<'_> {
    fn load_reserved(&mut self, _: u64, address: u64, width: Width) -> Result<u64, BusError> {
        self.read(address, width)
    }
    fn store_conditional(&mut self, _: u64, address: u64, width: Width, value: u64) -> Result<bool, BusError> {
        self.write(address, width, value)?;
        Ok(true)
    }
}

#[test]
fn data_and_management_wait_but_posted_commands_and_fetch_do_not() {
    // LD, SD, C.LD, C.SD, AMOSWAP.D, management, posted MSET, ADDI, and an unattached LD.
    for (instruction, ready, waits) in [
        (0x0005b503, 100, true),
        (0x00a5b023, 100, true),
        (0x00006188, 100, true),
        (0x0000e188, 100, true),
        (0x08a5b52f, 100, true),
        (0x1a00702b, 100, true),
        (0x4000307b, 100, false),
        (0x00100513, 100, false),
        (0x0005b503, 0, false),
    ] {
        let now = Cell::new(13);
        let waited = Cell::new(0);
        let mut memory = Memory {
            instruction,
            now: &now,
            expected_access_cycle: if waits { 100 } else { 13 },
            value: 42,
        };
        let mut registers = Registers::new(0, 0);
        registers.x[11] = 256;
        registers.x[10] = 99;
        let result = registers.step(
            &mut memory,
            &Mmu::default(),
            &Clock {
                now: &now,
                ready,
                waited: &waited,
            },
            3,
        );
        assert!(
            matches!(result, Step::Retired { .. } | Step::Custom(_)),
            "{instruction:x}: {result:?}"
        );
        assert_eq!(now.get(), if waits { 100 } else { 13 }, "{instruction:x}");
        assert_eq!(waited.get(), if waits { 87 } else { 0 });
        assert_eq!(registers.csrs.cycle, 3 + waited.get());
    }
}
