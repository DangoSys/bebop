use crate::execute::{self, Accelerator};
use generic::riscv::{
    bus::{Bus, BusError, HartBus, Width},
    input::Inputs,
    mmu::Mmu,
    registers::{Registers, Step},
};
use generic::{
    guest::GuestAccess,
    hlink::Port,
    stores::{LocalMemory, Stores},
    tasks::{Context, Event, Record},
    tlink::TLink,
};
use std::sync::{atomic::Ordering, Mutex};

struct LocalBus<'a> {
    code: &'a LocalMemory,
    tls: &'a mut LocalMemory,
    tss: &'a Mutex<LocalMemory>,
}
impl Bus for LocalBus<'_> {
    fn read(&mut self, address: u64, width: Width) -> Result<u64, BusError> {
        let n = width as usize;
        if self.code.offset(address, n).is_some() {
            self.code.read(address, n)
        } else if self.tls.offset(address, n).is_some() {
            self.tls.read(address, n)
        } else {
            self.tss.lock().unwrap().read(address, n)
        }
    }
    fn write(&mut self, address: u64, width: Width, value: u64) -> Result<(), BusError> {
        if self.tls.offset(address, width as usize).is_some() {
            self.tls.write(address, width as usize, value)
        } else {
            self.tss.lock().unwrap().write(address, width as usize, value)
        }
    }
    fn peripheral(&mut self) -> bool {
        false
    }
    fn compare_exchange(&mut self, _: u64, _: Width, _: u64, _: u64) -> Result<u64, BusError> {
        panic!("Ant has no atomic instructions")
    }
}
impl HartBus for LocalBus<'_> {
    fn load_reserved(&mut self, _: u64, _: u64, _: Width) -> Result<u64, BusError> {
        panic!("Ant has no LR")
    }
    fn store_conditional(&mut self, _: u64, _: u64, _: Width, _: u64) -> Result<bool, BusError> {
        panic!("Ant has no SC")
    }
}
fn legal(instruction: u32) -> bool {
    let op = instruction & 127;
    let f3 = (instruction >> 12) & 7;
    let f7 = instruction >> 25;
    let word = op & 8 != 0;
    match op {
        0x37 | 0x17 | 0x6f | 0x7b => true,
        0x67 => f3 == 0,
        0x63 => f3 == 0 || f3 == 1 || f3 >= 4,
        0x03 => f3 <= 6,
        0x23 => f3 <= 3,
        0x0f => f3 == 0,
        // Zicsr: only `csrrs rd, mcycle, x0` (csrr rd, mcycle), in machine mode.
        0x73 => f3 == 2 && (instruction >> 15) & 31 == 0 && instruction >> 20 == 0xb00,
        0x13 | 0x1b => {
            let operation = !word || matches!(f3, 0 | 1 | 5);
            let shift = if word {
                f7 == 0 || (f3 == 5 && f7 == 32)
            } else {
                instruction >> 26 == 0 || (f3 == 5 && instruction >> 26 == 16)
            };
            operation && (!matches!(f3, 1 | 5) || shift)
        }
        0x33 | 0x3b => {
            if f7 == 1 {
                !word || f3 == 0 || f3 >= 4
            } else {
                (!word || matches!(f3, 0 | 1 | 5)) && (f7 == 0 || (f7 == 32 && matches!(f3, 0 | 5)))
            }
        }
        _ => false,
    }
}
/// The CPU of a hardware thread, for the task it runs.
pub(crate) struct Ant {
    pub(crate) cpu: Registers,
}

impl Ant {
    /// Runs the task ahead of its time, recording what the timing unit of the hart replays.
    /// The cycles of the CPU since its last record and the task's value, or None once the
    /// threads stop.
    pub(crate) fn execute(
        &mut self,
        stores: &mut Stores,
        link: &TLink,
        index: usize,
        context: &Context,
        accelerator: &mut Accelerator,
        end: u64,
        snapshot: &(generic::riscv::csr::Csrs, generic::riscv::Privilege),
    ) -> Option<(u64, u64)> {
        // The task owns a fixed translation context; Ant cannot change satp.
        let tasks = &link.tasks;
        let mmu = Mmu::default();
        let mut quantum = 0u8;
        let mut ant = 0;
        loop {
            if tasks.stopped.load(Ordering::Acquire) {
                return None;
            }
            if context.cancelled.load(Ordering::Acquire) {
                return Some((ant, 0));
            }
            let pc = self.cpu.pc;
            assert!(pc % 4 == 0 && pc + 4 <= end, "Ant fetch outside loaded code");
            let instruction = stores.code.read(pc, 4).unwrap() as u32;
            if instruction == 0x73 {
                assert_eq!(self.cpu.register(17), 0, "unsupported Ant ecall");
                return Some((ant + 4, self.cpu.register(10)));
            }
            let opcode = instruction & 127;
            assert!(legal(instruction), "unsupported Ant instruction {instruction:08x}");
            let mut shared = false;
            if matches!(opcode, 0x03 | 0x23) {
                let imm = if opcode == 3 {
                    (instruction as i32) >> 20
                } else {
                    (((instruction >> 25) << 5 | (instruction >> 7) & 31) as i32) << 20 >> 20
                };
                let address = self
                    .cpu
                    .register(((instruction >> 15) & 31) as usize)
                    .wrapping_add(imm as u64);
                let width = 1usize << ((instruction >> 12) & 3);
                shared = stores.tls.offset(address, width).is_none();
                assert_eq!(address % width as u64, 0);
                assert!(
                    stores.tls.offset(address, width).is_some()
                        || tasks.tss_range.is_some_and(|(base, bytes)| address >= base
                            && address
                                .checked_add(width as u64)
                                .is_some_and(|last| last <= base + bytes as u64)),
                    "Ant ordinary memory outside TLS/TSS"
                );
            }
            // Executor + LocalStore: fetch/read/execute takes four clocks.
            // LSU adds a request and a response (plus synchronous SRAM read).
            // Ordinary instruction costs exclude shared-store arbitration;
            // the timing unit adds the cycles of a fence, which waits on the
            // accelerator's event timeline.
            ant += match opcode {
                0x03 => 7,
                0x23 => 6,
                0x7b => 5,
                0x0f => 0,
                0x33 | 0x3b if instruction >> 25 == 1 => {
                    if (instruction >> 12) & 7 < 4 {
                        // Multiply performs eight radix-256 steps.
                        14
                    } else {
                        let word = opcode == 0x3b;
                        let mask = if word { u32::MAX as u64 } else { u64::MAX };
                        let a = self.cpu.register(((instruction >> 15) & 31) as usize) & mask;
                        let b = self.cpu.register(((instruction >> 20) & 31) as usize) & mask;
                        let signed = instruction & (1 << 12) == 0;
                        let minimum = (mask >> 1) + 1;
                        // DivRem bypasses iteration for zero and signed overflow.
                        6 + if b == 0 || (signed && a == minimum && b == mask) {
                            0
                        } else if word {
                            32
                        } else {
                            64
                        }
                    }
                }
                _ => 4,
            };
            if opcode == 0x0f {
                tasks.record(
                    index,
                    Record {
                        ant: std::mem::take(&mut ant),
                        event: Event::Fence,
                    },
                );
            }
            // The thread observes its time and the hart's shared store only where the
            // timing unit has brought its time to this instruction.
            if opcode == 0x73 {
                let record = Record {
                    ant: std::mem::take(&mut ant),
                    event: Event::Cycle,
                };
                self.cpu.csrs.cycle = tasks.sync(index, record)?.0;
            }
            if shared {
                let record = Record {
                    ant: std::mem::take(&mut ant),
                    event: Event::Shared,
                };
                tasks.sync(index, record)?;
            }
            let mut bus = LocalBus {
                code: &stores.code,
                tls: &mut stores.tls,
                tss: tasks.tss.as_ref().unwrap(),
            };
            let mut step = self.cpu.step(&mut bus, &mmu, &Inputs::default(), 0);
            if let Step::Custom(command) = step {
                assert_eq!(opcode, 0x7b);
                let (csrs, privilege) = snapshot;
                let privilege = if *privilege == generic::riscv::Privilege::Machine
                    && csrs.satp != generic::riscv::mmu::Satp::Bare
                {
                    generic::riscv::Privilege::Supervisor
                } else {
                    *privilege
                };
                let mstatus = csrs.read(0x300, generic::riscv::Privilege::Machine, 0, 0, 0).unwrap();
                // Every page an instruction's DMA touches is translated before
                // it has any effect: a page fault stalls the instruction until
                // the owner of the task resolves it, then the whole instruction
                // runs again from the owner's cycle.
                let value = loop {
                    let mut memory = GuestAccess::new(
                        Port {
                            cache: None,
                            devices: &link.platform,
                            memory: &link.memory,
                            peripheral: false,
                        },
                        &mmu,
                        generic::riscv::mmu::TranslationContext {
                            privilege,
                            satp: csrs.satp,
                            mstatus,
                            pmp: &csrs.pmp,
                        },
                        link,
                        index + 1,
                    );
                    memory.local = link.thread_memory.get().map(|memory| (memory.as_ref(), index));
                    match execute::execute(
                        accelerator,
                        memory,
                        (instruction >> 25) as u8,
                        if instruction & (1 << 13) != 0 { command.rs1 } else { 0 },
                        if instruction & (1 << 12) != 0 { command.rs2 } else { 0 },
                        pc,
                        index,
                        ant,
                    ) {
                        Ok(value) => break value,
                        Err(trap) if matches!(trap.cause, 13 | 15) => {
                            let record = Record {
                                ant,
                                event: Event::Fault(trap),
                            };
                            tasks.sync(index, record)?;
                            ant = 0;
                            if context.cancelled.load(Ordering::Acquire) {
                                return Some((0, 0));
                            }
                        }
                        Err(trap) => panic!("Ant NPU access failed: {trap:?}"),
                    }
                };
                ant = 0;
                step = self
                    .cpu
                    .complete_custom(Ok((instruction & (1 << 14) != 0).then_some(value)));
            }
            assert!(
                matches!(step, Step::Retired { .. }),
                "Ant self failed at {pc:x}: {step:?}"
            );
            // A thread that runs its CPU alone still reports its time to the hart.
            quantum = quantum.wrapping_add(1);
            if quantum & 31 == 0 && ant != 0 {
                tasks.record(
                    index,
                    Record {
                        ant: std::mem::take(&mut ant),
                        event: Event::Run,
                    },
                );
            }
        }
    }
}
