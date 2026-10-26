use crate::core::riscv::{
    bus::{Bus, BusError, HartBus, Width},
    hart::{Hart, Step},
    input::Inputs,
    mmu::Mmu,
};
use crate::{
    core::{
        memory::GuestAccess,
        tasks::{Context, Tasks},
        CoreState,
    },
    device::platform::Port,
    thread::execute::{self, Accelerator},
};
use std::sync::{atomic::Ordering, Arc, Mutex};

pub(crate) struct LocalMemory {
    pub(crate) base: u64,
    pub(crate) bytes: Vec<u8>,
}
impl LocalMemory {
    pub(crate) fn new(base: u64, bytes: usize) -> Self {
        Self {
            base,
            bytes: vec![0xa5; bytes],
        }
    }
    pub(crate) fn offset(&self, address: u64, count: usize) -> Option<usize> {
        let offset = address.checked_sub(self.base)?;
        (offset.checked_add(count as u64)? <= self.bytes.len() as u64).then_some(offset as usize)
    }
    pub(crate) fn read(&self, address: u64, count: usize) -> Result<u64, BusError> {
        let offset = self.offset(address, count).ok_or(BusError)?;
        let mut data = [0; 8];
        data[..count].copy_from_slice(&self.bytes[offset..offset + count]);
        Ok(u64::from_le_bytes(data))
    }
    pub(crate) fn write(&mut self, address: u64, count: usize, value: u64) -> Result<(), BusError> {
        let offset = self.offset(address, count).ok_or(BusError)?;
        self.bytes[offset..offset + count].copy_from_slice(&value.to_le_bytes()[..count]);
        Ok(())
    }
}
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
pub(crate) struct Ant {
    pub(crate) code: LocalMemory,
    pub(crate) tls: LocalMemory,
    pub(crate) cpu: Hart,
}

impl Ant {
    pub fn new(params: &crate::configs::AntConfig) -> Self {
        let tls = params.tls.as_ref().expect("Ant TLS parameters");
        Self {
            code: LocalMemory::new(0, params.code_bytes as usize),
            tls: LocalMemory::new(tls.base, tls.bytes as usize),
            cpu: Hart::new(0, 0),
        }
    }

    pub(crate) fn execute<const TRACE: bool>(
        &mut self,
        tasks: &Tasks,
        index: usize,
        context: &Context,
        accelerator: &mut Accelerator,
        core: &Arc<CoreState>,
        end: u64,
        snapshot: &(crate::core::riscv::csr::Csrs, crate::core::riscv::Privilege),
    ) -> (u64, u64) {
        let mut steps = 0;
        loop {
            if tasks.stopped.load(Ordering::Acquire) || context.cancelled.load(Ordering::Acquire) {
                return (0, steps);
            }
            let pc = self.cpu.pc;
            assert!(pc % 4 == 0 && pc + 4 <= end, "Ant fetch outside loaded code");
            let instruction = self.code.read(pc, 4).unwrap() as u32;
            if instruction == 0x73 {
                assert_eq!(self.cpu.register(17), 0, "unsupported Ant ecall");
                return (self.cpu.register(10), steps);
            }
            let opcode = instruction & 127;
            assert!(legal(instruction), "unsupported Ant instruction {instruction:08x}");
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
                assert_eq!(address % width as u64, 0);
                assert!(
                    self.tls.offset(address, width).is_some()
                        || tasks.tss_range.is_some_and(|(base, bytes)| address >= base
                            && address
                                .checked_add(width as u64)
                                .is_some_and(|last| last <= base + bytes as u64)),
                    "Ant ordinary memory outside TLS/TSS"
                );
            }
            let mmu = Mmu::default();
            let mut bus = LocalBus {
                code: &self.code,
                tls: &mut self.tls,
                tss: tasks.tss.as_ref().unwrap(),
            };
            let mut step = self.cpu.step(&mut bus, &mmu, Inputs::default());
            if let Step::Custom(command) = step {
                assert_eq!(opcode, 0x7b);
                let (csrs, privilege) = snapshot;
                let privilege = if *privilege == crate::core::riscv::Privilege::Machine
                    && csrs.satp != crate::core::riscv::mmu::Satp::Bare
                {
                    crate::core::riscv::Privilege::Supervisor
                } else {
                    *privilege
                };
                let translation = crate::core::riscv::mmu::TranslationContext {
                    privilege,
                    satp: csrs.satp,
                    mstatus: csrs
                        .read(0x300, crate::core::riscv::Privilege::Machine, 0, 0, 0)
                        .unwrap(),
                    pmp: &csrs.pmp,
                };
                let memory = GuestAccess::new(
                    Port {
                        devices: &core.platform,
                        memory: &core.memory,
                    },
                    &mmu,
                    translation,
                );
                let value = execute::execute(
                    accelerator,
                    memory,
                    (instruction >> 25) as u8,
                    if instruction & (1 << 13) != 0 { command.rs1 } else { 0 },
                    if instruction & (1 << 12) != 0 { command.rs2 } else { 0 },
                    pc,
                    steps,
                )
                .expect("Ant NPU access failed");
                step = self
                    .cpu
                    .complete_custom(Ok((instruction & (1 << 14) != 0).then_some(value)));
            }
            assert!(
                matches!(step, Step::Retired { .. }),
                "Ant self failed at {pc:x}: {step:?}"
            );
            if TRACE {
                steps += 1;
            }
            if accelerator.barrier_hit {
                if !tasks.rendezvous(index) {
                    return (0, steps);
                }
                accelerator.barrier_hit = false;
            }
        }
    }
}
