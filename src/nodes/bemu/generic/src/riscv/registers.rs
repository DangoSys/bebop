use crate::riscv::{
    bus::{Bus, HartBus, Width},
    compressed,
    csr::{Csrs, FS, INTERRUPTS, MIE, MPIE, MPP, MPRV, SIE, SPIE, SPP, TSR, TVM, TW},
    decoded::{Decoded, Op},
    input::Environment,
    mmu::{Mmu, TranslationContext},
    Access, Privilege, Trap,
};
use crate::timing::CpuTiming;
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CustomInstruction {
    pub instruction: u32,
    pub pc: u64,
    pub rs1: u64,
    pub rs2: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    Retired {
        pc: u64,
        instruction: u32,
        /// The instruction as its 4-byte form: itself, or what its compressed form stands for.
        decoded: u32,
        /// The register its rs1 field names, before the instruction (the base of a memory access).
        operand: u64,
    },
    Trap(Trap),
    Waiting,
    /// A custom instruction could not issue: the registers stays at it.
    Stalled,
    Custom(CustomInstruction),
}

/// How far a run of instructions goes.
pub struct Pace<'a> {
    /// The cycle of the instruction the run is at.
    pub clock: &'a Cell<u64>,
    /// The cycles from the last retired instruction to the next.
    pub instruction_cycles: u64,
    /// The cycle no instruction after the first is at or beyond.
    pub horizon: u64,
    /// The instructions the run may still take.
    pub limit: u64,
}

/// The decoded instructions the hart keeps, by the physical address they were fetched from: what an instruction is
/// does not depend on the translation that reached it.
const CODE: usize = 1 << 17;

#[derive(Clone, Copy)]
struct Code {
    address: u64,
    decoded: Decoded,
}

/// The page the hart fetches from, as its translation stood when the hart entered it: a fetch from the page under
/// the same translation needs no other. Only a page the PMP allows whole is kept.
#[derive(Clone, Copy)]
struct FetchPage {
    page: u64,
    privilege: Privilege,
    satp: crate::riscv::mmu::Satp,
    pmp_generation: u64,
    mmu_generation: u64,
    physical: u64,
}

impl FetchPage {
    const NONE: Self = Self {
        page: u64::MAX,
        privilege: Privilege::Machine,
        satp: crate::riscv::mmu::Satp::Bare,
        pmp_generation: 0,
        mmu_generation: 0,
        physical: 0,
    };
}

#[derive(Clone, Copy)]
struct TranslationPage {
    virtual_page: u64,
    physical_page: u64,
    privilege: Privilege,
    satp: crate::riscv::mmu::Satp,
    status: u64,
    pmp_generation: u64,
    mmu_generation: u64,
    pmp_covers_page: bool,
}

pub struct Registers {
    pub id: u64,
    pub pc: u64,
    pub privilege: Privilege,
    pub csrs: Csrs,
    pub x: [u64; 32],
    pub f: [u64; 32],
    /// The hart has half-precision loads, stores, moves and conversions.
    pub half_precision: bool,
    waiting: bool,
    custom: Option<CustomInstruction>,
    code: Box<[Code; CODE]>,
    fetched: FetchPage,
    /// The translations of pages, by access and a hash of the page.
    pages: [Cell<Option<TranslationPage>>; 768],
}

impl Registers {
    pub fn new(id: u64, reset_pc: u64) -> Self {
        Self {
            id,
            pc: reset_pc,
            privilege: Privilege::Machine,
            csrs: Csrs::default(),
            x: [0; 32],
            f: [0; 32],
            half_precision: false,
            waiting: false,
            custom: None,
            code: vec![
                Code {
                    address: u64::MAX,
                    decoded: Decoded::NONE,
                };
                CODE
            ]
            .into_boxed_slice()
            .try_into()
            .ok()
            .unwrap(),
            fetched: FetchPage::NONE,
            pages: [const { Cell::new(None) }; 768],
        }
    }

    pub fn register(&self, index: usize) -> u64 {
        self.x[index]
    }

    pub fn set_register(&mut self, index: usize, value: u64) {
        if index != 0 {
            self.x[index] = value;
        }
    }

    pub fn translation_context(&self) -> TranslationContext<'_> {
        TranslationContext {
            privilege: self.privilege,
            satp: self.csrs.satp,
            mstatus: self.csrs.status,
            pmp: &self.csrs.pmp,
        }
    }

    #[inline(always)]
    fn translate(
        &self,
        bus: &mut impl Bus,
        mmu: &Mmu,
        address: u64,
        width: Width,
        access: Access,
    ) -> Result<u64, Trap> {
        if address & (width as u64 - 1) != 0 {
            return Err(access.misaligned(address));
        }
        let page = address >> 12;
        let generation = mmu.generation();
        let slot = Self::slot(page, access);
        match self.pages[slot].get() {
            Some(cached)
                if cached.virtual_page == page
                    && cached.privilege == self.privilege
                    && cached.satp == self.csrs.satp
                    && cached.status == self.csrs.status
                    && cached.pmp_generation == self.csrs.pmp.generation()
                    && cached.mmu_generation == generation =>
            {
                let physical = cached.physical_page << 12 | (address & 0xfff);
                if !cached.pmp_covers_page {
                    let mode = self.translation_context().effective_privilege(access);
                    if !self.csrs.pmp.allows(physical, width, access, mode) {
                        return Err(access.fault(address));
                    }
                }
                Ok(physical)
            }
            _ => {
                let context = self.translation_context();
                let mode = context.effective_privilege(access);
                let physical = mmu.translate(bus, &context, address, width, access)?;
                self.pages[slot].set(Some(TranslationPage {
                    virtual_page: page,
                    physical_page: physical >> 12,
                    privilege: self.privilege,
                    satp: self.csrs.satp,
                    status: self.csrs.status,
                    pmp_generation: self.csrs.pmp.generation(),
                    mmu_generation: generation,
                    pmp_covers_page: self.csrs.pmp.allows_range(physical & !0xfff, 4096, access, mode),
                }));
                Ok(physical)
            }
        }
    }

    /// Where the translation of a page for an access is kept.
    #[inline(always)]
    fn slot(page: u64, access: Access) -> usize {
        let bucket = ((page ^ (page >> 5)) & 255) as usize;
        bucket * 3
            + match access {
                Access::Fetch => 0,
                Access::Load => 1,
                Access::Store => 2,
            }
    }

    /// The instruction at the PC.
    #[inline(always)]
    fn fetch(&mut self, bus: &mut impl Bus, mmu: &Mmu) -> Result<Decoded, Trap> {
        let page = &self.fetched;
        if page.page == self.pc >> 12
            && page.privilege == self.privilege
            && page.satp == self.csrs.satp
            && page.pmp_generation == self.csrs.pmp.generation()
            && page.mmu_generation == mmu.generation()
        {
            let address = page.physical | (self.pc & 0xfff);
            let code = &self.code[(address as usize >> 1) & (CODE - 1)];
            if code.address == address {
                return Ok(code.decoded);
            }
        }
        self.fetch_decoded(bus, mmu)
    }

    /// The instruction at the PC, translated, fetched from memory and decoded.
    #[inline(never)]
    fn fetch_decoded(&mut self, bus: &mut impl Bus, mmu: &Mmu) -> Result<Decoded, Trap> {
        let address = self.translate(bus, mmu, self.pc, Width::Half, Access::Fetch)?;
        let page = self.pc >> 12;
        let covered = self.pages[Self::slot(page, Access::Fetch)]
            .get()
            .unwrap()
            .pmp_covers_page;
        self.fetched = if covered {
            FetchPage {
                page,
                privilege: self.privilege,
                satp: self.csrs.satp,
                pmp_generation: self.csrs.pmp.generation(),
                mmu_generation: mmu.generation(),
                physical: address & !0xfff,
            }
        } else {
            FetchPage::NONE
        };
        let index = (address as usize >> 1) & (CODE - 1);
        if covered && self.code[index].address == address {
            return Ok(self.code[index].decoded);
        }
        let low = bus
            .fetch(address, Width::Half)
            .map_err(|_| Access::Fetch.fault(self.pc))? as u32;
        if low & 3 != 3 {
            let decoded = Decoded::new(low, compressed::decode(low as u16)?);
            self.code[index] = Code { address, decoded };
            return Ok(decoded);
        }
        // An instruction that runs into the next page depends on that page's translation too: it is not kept.
        let crossing = self.pc & 0xfff == 0xffe;
        let upper = if crossing {
            self.translate(bus, mmu, self.pc.wrapping_add(2), Width::Half, Access::Fetch)?
        } else {
            let upper = address + 2;
            if upper >> 56 != 0
                || (!covered && !self.csrs.pmp.allows(upper, Width::Half, Access::Fetch, self.privilege))
            {
                return Err(Access::Fetch.fault(self.pc.wrapping_add(2)));
            }
            upper
        };
        let high = bus
            .fetch(upper, Width::Half)
            .map_err(|_| Access::Fetch.fault(self.pc.wrapping_add(2)))? as u32;
        let raw = low | high << 16;
        let decoded = Decoded::new(raw, raw);
        if !crossing {
            self.code[index] = Code { address, decoded };
        }
        Ok(decoded)
    }

    /// One instruction, `cycles` after the last: an enabled pending interrupt, or the instruction at the PC.
    pub fn step(&mut self, bus: &mut impl HartBus, mmu: &Mmu, inputs: &impl Environment, cycles: u64) -> Step {
        assert!(
            self.custom.is_none(),
            "complete the pending custom instruction before stepping"
        );
        if self.csrs.inhibit & 1 == 0 {
            self.csrs.cycle = self.csrs.cycle.wrapping_add(cycles);
        }
        if let Some(step) = self.interrupt(inputs) {
            return step;
        }
        self.instruction(bus, mmu, inputs)
    }

    /// The instructions from the one `pace.clock` stands at, `cycles` after the last, each at the cycle `timing`
    /// gives it. Interrupts are taken before the first only, so the run ends with whatever can make one pending
    /// or enabled, or needs the hart around the registers: an instruction that does more than retire, a system or
    /// fence instruction, an access to a peripheral, an event of the environment, and the pace's limit and horizon.
    /// Returns the step of the last instruction; one that retired is timed, and `pace.instruction_cycles` are the
    /// cycles to the next.
    pub fn run(
        &mut self,
        bus: &mut impl HartBus,
        mmu: &Mmu,
        inputs: &impl Environment,
        timing: &mut dyn CpuTiming,
        cycles: u64,
        pace: &mut Pace<'_>,
    ) -> Step {
        let events = inputs.events();
        let mut step = self.step(bus, mmu, inputs, cycles);
        loop {
            pace.limit -= 1;
            let Step::Retired {
                pc,
                instruction,
                decoded,
                operand,
            } = step
            else {
                return step;
            };
            pace.instruction_cycles = timing.instruction(pc, instruction, decoded, operand);
            let cycle = pace.clock.get() + pace.instruction_cycles;
            if matches!(decoded & 0x7f, 0x0f | 0x73)
                || bus.peripheral()
                || inputs.events() != events
                || pace.limit == 0
                || cycle >= pace.horizon
            {
                return step;
            }
            pace.clock.set(cycle);
            if self.csrs.inhibit & 1 == 0 {
                self.csrs.cycle = self.csrs.cycle.wrapping_add(pace.instruction_cycles);
            }
            step = self.instruction(bus, mmu, inputs);
        }
    }

    /// The enabled pending interrupt the hart takes, or its wait for one.
    fn interrupt(&mut self, inputs: &impl Environment) -> Option<Step> {
        let pending = if self.csrs.ie & INTERRUPTS != 0 {
            (inputs.interrupts() | self.csrs.software_pending | self.csrs.dma_pending()) & self.csrs.ie & INTERRUPTS
        } else {
            0
        };
        if pending != 0 {
            self.waiting = false;
            for cause in [11, 3, 7, 9, 1, 5, 16] {
                let mask = 1 << cause;
                let delegated = self.csrs.ideleg & mask != 0;
                let enabled = if delegated {
                    self.privilege == Privilege::User
                        || (self.privilege == Privilege::Supervisor && self.csrs.status & SIE != 0)
                } else {
                    self.privilege != Privilege::Machine || self.csrs.status & MIE != 0
                };
                if pending & mask != 0 && enabled {
                    let trap = Trap {
                        cause: (1 << 63) | cause,
                        value: 0,
                    };
                    self.enter_trap(trap);
                    return Some(Step::Trap(trap));
                }
            }
        }
        self.waiting.then_some(Step::Waiting)
    }

    /// The instruction at the PC.
    #[inline(always)]
    fn instruction(&mut self, bus: &mut impl HartBus, mmu: &Mmu, inputs: &impl Environment) -> Step {
        let pc = self.pc;
        let code = match self.fetch(bus, mmu) {
            Ok(code) => code,
            Err(trap) => {
                self.enter_trap(trap);
                return Step::Trap(trap);
            }
        };
        let operand = self.x[code.rs1 as usize];
        let a = operand;
        let b = self.x[code.rs2 as usize];
        let imm = code.imm as u64;
        let next = pc.wrapping_add(if code.raw & 3 == 3 { 4 } else { 2 });
        let word = |value: u32| Ok((value as i32 as u64, next));
        // The register value and the next PC.
        let result = match code.op {
            Op::Other => return self.other(bus, mmu, inputs, code, next),
            Op::Lui => Ok((imm, next)),
            Op::Auipc => Ok((pc.wrapping_add(imm), next)),
            Op::Jal => Ok((next, pc.wrapping_add(imm))),
            Op::Jalr => Ok((next, a.wrapping_add(imm) & !1)),
            Op::Beq => Ok((0, if a == b { pc.wrapping_add(imm) } else { next })),
            Op::Bne => Ok((0, if a != b { pc.wrapping_add(imm) } else { next })),
            Op::Blt => Ok((
                0,
                if (a as i64) < b as i64 {
                    pc.wrapping_add(imm)
                } else {
                    next
                },
            )),
            Op::Bge => Ok((
                0,
                if (a as i64) >= b as i64 {
                    pc.wrapping_add(imm)
                } else {
                    next
                },
            )),
            Op::Bltu => Ok((0, if a < b { pc.wrapping_add(imm) } else { next })),
            Op::Bgeu => Ok((0, if a >= b { pc.wrapping_add(imm) } else { next })),
            Op::Lb | Op::Lh | Op::Lw | Op::Ld | Op::Lbu | Op::Lhu | Op::Lwu => {
                self.wait_memory(inputs);
                let width = match code.op {
                    Op::Lb | Op::Lbu => Width::Byte,
                    Op::Lh | Op::Lhu => Width::Half,
                    Op::Lw | Op::Lwu => Width::Word,
                    _ => Width::Double,
                };
                self.load(bus, mmu, a.wrapping_add(imm), width, Access::Load)
                    .map(|value| {
                        let value = match code.op {
                            Op::Lb => value as i8 as u64,
                            Op::Lh => value as i16 as u64,
                            Op::Lw => value as i32 as u64,
                            _ => value,
                        };
                        (value, next)
                    })
            }
            Op::Sb | Op::Sh | Op::Sw | Op::Sd => {
                self.wait_memory(inputs);
                let width = match code.op {
                    Op::Sb => Width::Byte,
                    Op::Sh => Width::Half,
                    Op::Sw => Width::Word,
                    _ => Width::Double,
                };
                self.store(bus, mmu, a.wrapping_add(imm), width, b).map(|()| (0, next))
            }
            Op::Addi => Ok((a.wrapping_add(imm), next)),
            Op::Slti => Ok((u64::from((a as i64) < imm as i64), next)),
            Op::Sltiu => Ok((u64::from(a < imm), next)),
            Op::Xori => Ok((a ^ imm, next)),
            Op::Ori => Ok((a | imm, next)),
            Op::Andi => Ok((a & imm, next)),
            Op::Slli => Ok((a << imm, next)),
            Op::Srli => Ok((a >> imm, next)),
            Op::Srai => Ok((((a as i64) >> imm) as u64, next)),
            Op::Addiw => word(a.wrapping_add(imm) as u32),
            Op::Slliw => word((a as u32) << imm),
            Op::Srliw => word((a as u32) >> imm),
            Op::Sraiw => word(((a as i32) >> imm) as u32),
            Op::Add => Ok((a.wrapping_add(b), next)),
            Op::Sub => Ok((a.wrapping_sub(b), next)),
            Op::Sll => Ok((a << (b & 63), next)),
            Op::Slt => Ok((u64::from((a as i64) < b as i64), next)),
            Op::Sltu => Ok((u64::from(a < b), next)),
            Op::Xor => Ok((a ^ b, next)),
            Op::Srl => Ok((a >> (b & 63), next)),
            Op::Sra => Ok((((a as i64) >> (b & 63)) as u64, next)),
            Op::Or => Ok((a | b, next)),
            Op::And => Ok((a & b, next)),
            Op::Addw => word((a as u32).wrapping_add(b as u32)),
            Op::Subw => word((a as u32).wrapping_sub(b as u32)),
            Op::Sllw => word((a as u32) << (b & 31)),
            Op::Srlw => word((a as u32) >> (b & 31)),
            Op::Sraw => word(((a as i32) >> (b & 31)) as u32),
        };
        match result {
            Ok((value, next)) => {
                // A branch and a store leave the registers alone.
                if !matches!(
                    code.op,
                    Op::Beq | Op::Bne | Op::Blt | Op::Bge | Op::Bltu | Op::Bgeu | Op::Sb | Op::Sh | Op::Sw | Op::Sd
                ) {
                    self.set_register(code.rd as usize, value);
                }
                self.pc = next;
                if self.csrs.inhibit & 4 == 0 {
                    self.csrs.instret = self.csrs.instret.wrapping_add(1);
                }
                Step::Retired {
                    pc,
                    instruction: code.raw,
                    decoded: code.insn,
                    operand,
                }
            }
            Err(trap) => {
                self.enter_trap(trap);
                Step::Trap(trap)
            }
        }
    }

    /// A data access first drains the accelerator work attached to memory.
    #[inline(always)]
    fn wait_memory(&mut self, inputs: &impl Environment) {
        let cycles = inputs.memory_wait_cycles();
        if self.csrs.inhibit & 1 == 0 {
            self.csrs.cycle = self.csrs.cycle.wrapping_add(cycles);
        }
    }

    /// An instruction executed from its bits.
    #[inline(never)]
    fn other(
        &mut self,
        bus: &mut impl HartBus,
        mmu: &Mmu,
        inputs: &impl Environment,
        code: Decoded,
        next: u64,
    ) -> Step {
        let pc = self.pc;
        let instruction = code.insn;
        if matches!(instruction & 0x7f, 0x03 | 0x07 | 0x23 | 0x27 | 0x2f | 0x2b) {
            self.wait_memory(inputs);
        }
        let operand = self.x[code.rs1 as usize];
        if matches!(instruction & 0x7f, 0x0b | 0x2b | 0x5b | 0x7b) {
            let request = CustomInstruction {
                instruction,
                pc,
                rs1: operand,
                rs2: self.x[code.rs2 as usize],
            };
            self.custom = Some(request);
            return Step::Custom(request);
        }
        self.csrs.instret_written = false;
        let count_retired = self.csrs.inhibit & 4 == 0;
        match self.execute(bus, mmu, inputs, instruction, next) {
            Ok(next_pc) => {
                self.pc = next_pc;
                if count_retired && !self.csrs.instret_written {
                    self.csrs.instret = self.csrs.instret.wrapping_add(1);
                }
                Step::Retired {
                    pc,
                    instruction: code.raw,
                    decoded: instruction,
                    operand,
                }
            }
            Err(mut trap) => {
                if trap.cause == 2 {
                    trap.value = code.raw.into();
                }
                self.enter_trap(trap);
                Step::Trap(trap)
            }
        }
    }

    /// The custom instruction cannot issue while the DMA of the accelerator is
    /// stalled: the registers stays at it, without effect, and runs it again once
    /// it returns to it. It takes interrupt 16 when enabled, or waits for
    /// another interrupt.
    pub fn stall_custom(&mut self) -> Step {
        self.custom.take().expect("no custom instruction is pending");
        if self.csrs.ie & self.csrs.dma_pending() != 0 {
            Step::Stalled
        } else {
            self.waiting = true;
            Step::Waiting
        }
    }

    // The external extension supplies an optional integer result or an architectural trap.
    pub fn complete_custom(&mut self, result: Result<Option<u64>, Trap>) -> Step {
        let request = self.custom.take().expect("no custom instruction is pending");
        match result {
            Ok(value) => {
                if let Some(value) = value {
                    self.set_register(((request.instruction >> 7) & 31) as usize, value);
                }
                self.pc = request.pc.wrapping_add(4);
                if self.csrs.inhibit & 4 == 0 {
                    self.csrs.instret = self.csrs.instret.wrapping_add(1);
                }
                Step::Retired {
                    pc: request.pc,
                    instruction: request.instruction,
                    decoded: request.instruction,
                    operand: request.rs1,
                }
            }
            Err(trap) => {
                self.enter_trap(trap);
                Step::Trap(trap)
            }
        }
    }

    pub fn return_from_machine_trap(&mut self) {
        assert_eq!(self.privilege, Privilege::Machine);
        self.privilege = match (self.csrs.status & MPP) >> 11 {
            0 => Privilege::User,
            1 => Privilege::Supervisor,
            3 => Privilege::Machine,
            _ => unreachable!(),
        };
        let enable = if self.csrs.status & MPIE != 0 { MIE } else { 0 };
        self.csrs.status = (self.csrs.status & !(MIE | MPIE | MPP)) | enable | MPIE;
        if self.privilege != Privilege::Machine {
            self.csrs.status &= !MPRV;
        }
        self.pc = self.csrs.epc[1];
    }

    fn enter_trap(&mut self, trap: Trap) {
        let interrupt = trap.cause >> 63 != 0;
        let cause = trap.cause & !(1 << 63);
        let delegate = if interrupt { self.csrs.ideleg } else { self.csrs.edeleg };
        let supervisor = self.privilege != Privilege::Machine && cause < 64 && delegate & (1 << cause) != 0;
        let target = usize::from(!supervisor);
        self.csrs.epc[target] = self.pc & !1;
        self.csrs.cause[target] = trap.cause;
        self.csrs.tval[target] = trap.value;
        if supervisor {
            let saved = if self.csrs.status & SIE != 0 { SPIE } else { 0 };
            let previous = if self.privilege == Privilege::Supervisor {
                SPP
            } else {
                0
            };
            self.csrs.status = (self.csrs.status & !(SIE | SPIE | SPP)) | saved | previous;
            self.privilege = Privilege::Supervisor;
        } else {
            let saved = if self.csrs.status & MIE != 0 { MPIE } else { 0 };
            self.csrs.status = (self.csrs.status & !(MIE | MPIE | MPP)) | saved | ((self.privilege as u64) << 11);
            self.privilege = Privilege::Machine;
        }
        self.pc = self.csrs.tvec[target] & !3;
        if interrupt && self.csrs.tvec[target] & 3 == 1 {
            self.pc = self.pc.wrapping_add(4 * cause);
        }
        self.waiting = false;
    }

    pub fn load(&self, bus: &mut impl Bus, mmu: &Mmu, address: u64, width: Width, access: Access) -> Result<u64, Trap> {
        if access == Access::Load && address & (width as u64 - 1) != 0 {
            let mut value = 0;
            for byte in 0..width as u64 {
                let virtual_address = address.wrapping_add(byte);
                let physical = self.translate(bus, mmu, virtual_address, Width::Byte, access)?;
                value |= bus
                    .read(physical, Width::Byte)
                    .map_err(|_| access.fault(virtual_address))?
                    << (8 * byte);
            }
            return Ok(value);
        }
        let physical = self.translate(bus, mmu, address, width, access)?;
        bus.read(physical, width).map_err(|_| access.fault(address))
    }

    pub fn store(&self, bus: &mut impl Bus, mmu: &Mmu, address: u64, width: Width, value: u64) -> Result<(), Trap> {
        // Ordinary loads/stores support misalignment. AMOs and LR/SC still require alignment.
        if address & (width as u64 - 1) != 0 {
            for byte in 0..width as u64 {
                let virtual_address = address.wrapping_add(byte);
                let physical = self.translate(bus, mmu, virtual_address, Width::Byte, Access::Store)?;
                bus.write(physical, Width::Byte, (value >> (8 * byte)) & 0xff)
                    .map_err(|_| Access::Store.fault(virtual_address))?;
            }
            return Ok(());
        }
        let physical = self.translate(bus, mmu, address, width, Access::Store)?;
        bus.write(physical, width, value)
            .map_err(|_| Access::Store.fault(address))
    }

    /// An instruction no decoded operation stands for. Returns the next PC.
    fn execute(
        &mut self,
        bus: &mut impl HartBus,
        mmu: &Mmu,
        inputs: &impl Environment,
        insn: u32,
        next: u64,
    ) -> Result<u64, Trap> {
        let opcode = insn & 0x7f;
        let rd = ((insn >> 7) & 31) as usize;
        let funct = (insn >> 12) & 7;
        let rs1 = ((insn >> 15) & 31) as usize;
        let rs2 = ((insn >> 20) & 31) as usize;
        let top = insn >> 25;
        let a = self.x[rs1];
        let b = self.x[rs2];
        let illegal = Trap::illegal(insn);
        if top != 0 && matches!(opcode, 0x13 | 0x1b | 0x33 | 0x3b) {
            if let Some(value) = crate::riscv::bitmanip::execute(insn, a, b, 64) {
                self.set_register(rd, value);
                return Ok(next);
            }
        }
        let value = match opcode {
            0x33 | 0x3b => {
                let word = opcode == 0x3b;
                let a = if word { a as i32 as u64 } else { a };
                let b = if word { b as i32 as u64 } else { b };
                let value = match (top, funct) {
                    (1, 0) => a.wrapping_mul(b),
                    (1, 1) if !word => (((a as i64 as i128) * (b as i64 as i128)) >> 64) as u64,
                    (1, 2) if !word => (((a as i64 as i128) * (b as i128)) >> 64) as u64,
                    (1, 3) if !word => (((a as u128) * (b as u128)) >> 64) as u64,
                    (1, 4) => {
                        if b == 0 {
                            u64::MAX
                        } else {
                            (a as i64).wrapping_div(b as i64) as u64
                        }
                    }
                    (1, 6) => {
                        if b == 0 {
                            a
                        } else {
                            (a as i64).wrapping_rem(b as i64) as u64
                        }
                    }
                    (1, 5 | 7) => {
                        let a = if word { a as u32 as u64 } else { a };
                        let b = if word { b as u32 as u64 } else { b };
                        if funct == 5 {
                            // RISC-V defines division by zero to return all ones.
                            a.checked_div(b).unwrap_or(u64::MAX)
                        } else if b == 0 {
                            a
                        } else {
                            a % b
                        }
                    }
                    _ => return Err(illegal),
                };
                if word {
                    value as i32 as u64
                } else {
                    value
                }
            }
            0x0f if funct == 0 => return Ok(next),
            0x0f if funct == 1 => {
                self.code.fill(Code {
                    address: u64::MAX,
                    decoded: Decoded::NONE,
                });
                self.fetched = FetchPage::NONE;
                return Ok(next);
            }
            0x2f => self.atomic(bus, mmu, insn, a, b)?,
            0x73 => {
                if funct == 0 {
                    match insn {
                        0x00000073 => {
                            return Err(Trap {
                                cause: 8 + self.privilege as u64,
                                value: 0,
                            })
                        }
                        0x00100073 => {
                            return Err(Trap {
                                cause: 3,
                                value: self.pc,
                            })
                        }
                        0x30200073 if self.privilege == Privilege::Machine => {
                            self.return_from_machine_trap();
                            return Ok(self.pc);
                        }
                        0x10200073
                            if self.privilege != Privilege::User
                                && (self.privilege == Privilege::Machine || self.csrs.status & TSR == 0) =>
                        {
                            self.privilege = if self.csrs.status & SPP != 0 {
                                Privilege::Supervisor
                            } else {
                                Privilege::User
                            };
                            let enable = if self.csrs.status & SPIE != 0 { SIE } else { 0 };
                            self.csrs.status = (self.csrs.status & !(SIE | SPIE | SPP | MPRV)) | enable | SPIE;
                            return Ok(self.csrs.epc[0]);
                        }
                        0x10500073
                            if self.privilege != Privilege::User
                                && (self.privilege == Privilege::Machine || self.csrs.status & TW == 0) =>
                        {
                            self.waiting = true;
                            return Ok(next);
                        }
                        _ if insn & 0xfe007fff == 0x12000073
                            && self.privilege != Privilege::User
                            && (self.privilege == Privilege::Machine || self.csrs.status & TVM == 0) =>
                        {
                            mmu.fence();
                            return Ok(next);
                        }
                        _ => return Err(illegal),
                    }
                }
                if funct == 4 {
                    return Err(illegal);
                }
                let address = (insn >> 20) as u16;
                let operand = if funct & 4 != 0 { rs1 as u64 } else { a };
                let old = self
                    .csrs
                    .read(
                        address,
                        self.privilege,
                        self.id,
                        if address == 0xc01 { inputs.time() } else { 0 },
                        if matches!(address, 0x144 | 0x344) {
                            inputs.interrupts()
                        } else {
                            0
                        },
                    )
                    .map_err(|_| illegal)?;
                let write = funct & 3 == 1 || rs1 != 0;
                if write {
                    let baseline = match address {
                        0x344 => self.csrs.software_pending,
                        0x144 => self.csrs.software_pending & self.csrs.ideleg,
                        _ => old,
                    };
                    let value = match funct & 3 {
                        1 => operand,
                        2 => baseline | operand,
                        3 => baseline & !operand,
                        _ => unreachable!(),
                    };
                    self.csrs.write(address, value, self.privilege).map_err(|_| illegal)?;
                }
                old
            }
            0x07 | 0x27 | 0x43 | 0x47 | 0x4b | 0x4f | 0x53 => {
                if self.csrs.status & FS == 0 {
                    return Err(illegal);
                }
                self.execute_float(bus, mmu, insn)?;
                return Ok(next);
            }
            _ => return Err(illegal),
        };
        self.set_register(rd, value);
        Ok(next)
    }

    fn atomic(&self, bus: &mut impl HartBus, mmu: &Mmu, insn: u32, address: u64, value: u64) -> Result<u64, Trap> {
        let width = match (insn >> 12) & 7 {
            2 => Width::Word,
            3 => Width::Double,
            _ => return Err(Trap::illegal(insn)),
        };
        let op = insn >> 27;
        if !matches!(op, 0 | 1 | 2 | 3 | 4 | 8 | 12 | 16 | 20 | 24 | 28) || (op == 2 && (insn >> 20) & 31 != 0) {
            return Err(Trap::illegal(insn));
        }
        let access = if op == 2 { Access::Load } else { Access::Store };
        let physical = self.translate(bus, mmu, address, width, access)?;
        if op == 2 {
            let old = bus
                .load_reserved(self.id, physical, width)
                .map_err(|_| access.fault(address))?;
            return Ok(if width == Width::Word { old as i32 as u64 } else { old });
        }
        if op == 3 {
            let success = bus
                .store_conditional(self.id, physical, width, value)
                .map_err(|_| access.fault(address))?;
            return Ok(u64::from(!success));
        }
        let mask = if width == Width::Word {
            u32::MAX as u64
        } else {
            u64::MAX
        };
        let value = value & mask;
        let mut old = bus.read(physical, width).map_err(|_| access.fault(address))?;
        loop {
            let signed_old = if width == Width::Word {
                old as i32 as i64
            } else {
                old as i64
            };
            let signed_value = if width == Width::Word {
                value as i32 as i64
            } else {
                value as i64
            };
            let new = match op {
                0 => old.wrapping_add(value),
                1 => value,
                4 => old ^ value,
                8 => old | value,
                12 => old & value,
                16 => signed_old.min(signed_value) as u64,
                20 => signed_old.max(signed_value) as u64,
                24 => old.min(value),
                28 => old.max(value),
                _ => unreachable!(),
            } & mask;
            let observed = bus
                .compare_exchange(physical, width, old, new)
                .map_err(|_| access.fault(address))?;
            if observed == old {
                return Ok(if width == Width::Word { old as i32 as u64 } else { old });
            }
            old = observed;
        }
    }
}
