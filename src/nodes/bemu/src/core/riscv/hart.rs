use crate::core::riscv::{
    bus::{Bus, HartBus, Width},
    compressed,
    csr::{Csrs, FS, INTERRUPTS, MIE, MPIE, MPP, MPRV, SIE, SPIE, SPP, TSR, TVM, TW},
    input::Environment,
    mmu::{Mmu, TranslationContext},
    Access, Privilege, Trap,
};
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
    Retired { pc: u64, instruction: u32 },
    Trap(Trap),
    Waiting,
    Custom(CustomInstruction),
}

#[derive(Clone, Copy)]
struct Instruction {
    address: u64,
    upper_address: u64,
    raw: u32,
    decoded: u32,
    length: u64,
}

#[derive(Clone, Copy)]
struct TranslationPage {
    virtual_page: u64,
    physical_page: u64,
    privilege: Privilege,
    satp: crate::core::riscv::mmu::Satp,
    status: u64,
    pmp_generation: u64,
    mmu_generation: u64,
    pmp_covers_page: bool,
}

pub struct Hart {
    pub id: u64,
    pub pc: u64,
    pub privilege: Privilege,
    pub csrs: Csrs,
    pub(crate) x: [u64; 32],
    pub f: [u64; 32],
    waiting: bool,
    custom: Option<CustomInstruction>,
    instructions: Box<[Option<Instruction>; 1024]>,
    pages: [Cell<Option<TranslationPage>>; 513],
}

impl Hart {
    pub fn new(id: u64, reset_pc: u64) -> Self {
        Self {
            id,
            pc: reset_pc,
            privilege: Privilege::Machine,
            csrs: Csrs::default(),
            x: [0; 32],
            f: [0; 32],
            waiting: false,
            custom: None,
            instructions: Box::new([None; 1024]),
            pages: [const { Cell::new(None) }; 513],
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
        let bucket = ((page ^ (page >> 5)) & 255) as usize;
        let slot = match access {
            Access::Fetch => 0,
            Access::Load => 1 + bucket * 2,
            Access::Store => 2 + bucket * 2,
        };
        let context = self.translation_context();
        let mode = context.effective_privilege(access);
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
                if !cached.pmp_covers_page && !self.csrs.pmp.allows(physical, width, access, mode) {
                    return Err(access.fault(address));
                }
                Ok(physical)
            }
            _ => {
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

    fn fetch(&mut self, bus: &mut impl Bus, mmu: &Mmu) -> Result<Instruction, Trap> {
        let address = self.translate(bus, mmu, self.pc, Width::Half, Access::Fetch)?;
        let slot = (address as usize >> 1) & (self.instructions.len() - 1);
        if let Some(instruction) = self.instructions[slot] {
            if instruction.address == address {
                if instruction.length == 2 {
                    return Ok(instruction);
                }
                let upper = if self.pc & 0xfff == 0xffe {
                    self.translate(bus, mmu, self.pc.wrapping_add(2), Width::Half, Access::Fetch)?
                } else {
                    let upper = address + 2;
                    if upper >> 56 != 0
                        || (!self.pages[0].get().unwrap().pmp_covers_page
                            && !self.csrs.pmp.allows(upper, Width::Half, Access::Fetch, self.privilege))
                    {
                        return Err(Access::Fetch.fault(self.pc.wrapping_add(2)));
                    }
                    upper
                };
                if instruction.upper_address == upper {
                    return Ok(instruction);
                }
            }
        }
        let low = bus
            .read(address, Width::Half)
            .map_err(|_| Access::Fetch.fault(self.pc))? as u32;
        let instruction = if low & 3 != 3 {
            Instruction {
                address,
                upper_address: address,
                raw: low,
                decoded: compressed::decode(low as u16)?,
                length: 2,
            }
        } else {
            let upper_address = if self.pc & 0xfff == 0xffe {
                self.translate(bus, mmu, self.pc.wrapping_add(2), Width::Half, Access::Fetch)?
            } else {
                let upper = address + 2;
                if upper >> 56 != 0
                    || (!self.pages[0].get().unwrap().pmp_covers_page
                        && !self.csrs.pmp.allows(upper, Width::Half, Access::Fetch, self.privilege))
                {
                    return Err(Access::Fetch.fault(self.pc.wrapping_add(2)));
                }
                upper
            };
            let high = bus
                .read(upper_address, Width::Half)
                .map_err(|_| Access::Fetch.fault(self.pc.wrapping_add(2)))? as u32;
            let raw = low | high << 16;
            Instruction {
                address,
                upper_address,
                raw,
                decoded: raw,
                length: 4,
            }
        };
        self.instructions[slot] = Some(instruction);
        Ok(instruction)
    }

    pub fn step(&mut self, bus: &mut impl HartBus, mmu: &Mmu, inputs: impl Environment) -> Step {
        assert!(
            self.custom.is_none(),
            "complete the pending custom instruction before stepping"
        );
        let cycles = inputs.cycles();
        if self.csrs.inhibit & 1 == 0 {
            self.csrs.cycle = self.csrs.cycle.wrapping_add(cycles);
        }
        let pending = if self.csrs.ie & INTERRUPTS != 0 {
            (inputs.interrupts() | self.csrs.software_pending) & self.csrs.ie & INTERRUPTS
        } else {
            0
        };
        if pending != 0 {
            self.waiting = false;
            for cause in [11, 3, 7, 9, 1, 5] {
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
                    return Step::Trap(trap);
                }
            }
        }
        if self.waiting {
            return Step::Waiting;
        }
        let pc = self.pc;
        let (raw, instruction, length) = match self.fetch(bus, mmu) {
            Ok(fetched) => (fetched.raw, fetched.decoded, fetched.length),
            Err(trap) => {
                self.enter_trap(trap);
                return Step::Trap(trap);
            }
        };
        if matches!(instruction & 0x7f, 0x0b | 0x2b | 0x5b | 0x7b) {
            let request = CustomInstruction {
                instruction,
                pc,
                rs1: self.x[((instruction >> 15) & 31) as usize],
                rs2: self.x[((instruction >> 20) & 31) as usize],
            };
            self.custom = Some(request);
            return Step::Custom(request);
        }
        self.csrs.instret_written = false;
        let count_retired = self.csrs.inhibit & 4 == 0;
        match self.execute(bus, mmu, &inputs, instruction, pc.wrapping_add(length)) {
            Ok(next_pc) => {
                self.pc = next_pc;
                if count_retired && !self.csrs.instret_written {
                    self.csrs.instret = self.csrs.instret.wrapping_add(1);
                }
                Step::Retired { pc, instruction: raw }
            }
            Err(mut trap) => {
                if trap.cause == 2 {
                    trap.value = raw.into();
                }
                self.enter_trap(trap);
                Step::Trap(trap)
            }
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

    pub(crate) fn load(
        &self,
        bus: &mut impl Bus,
        mmu: &Mmu,
        address: u64,
        width: Width,
        access: Access,
    ) -> Result<u64, Trap> {
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

    pub(crate) fn store(
        &self,
        bus: &mut impl Bus,
        mmu: &Mmu,
        address: u64,
        width: Width,
        value: u64,
    ) -> Result<(), Trap> {
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

    fn execute(
        &mut self,
        bus: &mut impl HartBus,
        mmu: &Mmu,
        inputs: &impl Environment,
        insn: u32,
        mut next: u64,
    ) -> Result<u64, Trap> {
        let opcode = insn & 0x7f;
        let rd = ((insn >> 7) & 31) as usize;
        let funct = (insn >> 12) & 7;
        let rs1 = ((insn >> 15) & 31) as usize;
        let rs2 = ((insn >> 20) & 31) as usize;
        let top = insn >> 25;
        let a = self.x[rs1];
        let b = self.x[rs2];
        let imm = ((insn as i32) >> 20) as u64;
        let illegal = Trap::illegal(insn);
        if let Some(value) = crate::core::riscv::bitmanip::execute(insn, a, b, 64) {
            self.set_register(rd, value);
            return Ok(next);
        }
        let value = match opcode {
            0x37 => (insn & 0xfffff000) as i32 as u64,
            0x17 => self.pc.wrapping_add((insn & 0xfffff000) as i32 as u64),
            0x6f => {
                let offset =
                    (insn & 0xff000) | ((insn >> 9) & 0x800) | ((insn >> 20) & 0x7fe) | ((insn >> 11) & 0x100000);
                let target = self.pc.wrapping_add(((offset << 11) as i32 >> 11) as u64);
                let link = next;
                next = target;
                link
            }
            0x67 if funct == 0 => {
                let link = next;
                next = a.wrapping_add(imm) & !1;
                link
            }
            0x63 => {
                let take = match funct {
                    0 => a == b,
                    1 => a != b,
                    4 => (a as i64) < b as i64,
                    5 => (a as i64) >= b as i64,
                    6 => a < b,
                    7 => a >= b,
                    _ => return Err(illegal),
                };
                if take {
                    let offset =
                        ((insn >> 19) & 0x1000) | ((insn << 4) & 0x800) | ((insn >> 20) & 0x7e0) | ((insn >> 7) & 0x1e);
                    next = self.pc.wrapping_add(((offset << 19) as i32 >> 19) as u64);
                }
                return Ok(next);
            }
            0x03 => {
                let address = a.wrapping_add(imm);
                let width = match funct {
                    0 | 4 => Width::Byte,
                    1 | 5 => Width::Half,
                    2 | 6 => Width::Word,
                    3 => Width::Double,
                    _ => return Err(illegal),
                };
                let value = self.load(bus, mmu, address, width, Access::Load)?;
                match funct {
                    0 => value as i8 as u64,
                    1 => value as i16 as u64,
                    2 => value as i32 as u64,
                    _ => value,
                }
            }
            0x23 => {
                let offset = (((insn >> 7) & 31) | ((insn >> 20) & 0xfe0)) as i32;
                let address = a.wrapping_add(((offset << 20) >> 20) as u64);
                let width = match funct {
                    0 => Width::Byte,
                    1 => Width::Half,
                    2 => Width::Word,
                    3 => Width::Double,
                    _ => return Err(illegal),
                };
                self.store(bus, mmu, address, width, b)?;
                return Ok(next);
            }
            0x13 => match funct {
                0 => a.wrapping_add(imm),
                2 => u64::from((a as i64) < imm as i64),
                3 => u64::from(a < imm),
                4 => a ^ imm,
                6 => a | imm,
                7 => a & imm,
                1 if insn >> 26 == 0 => a << (imm & 63),
                5 if insn >> 26 == 0 => a >> (imm & 63),
                5 if insn >> 26 == 0x10 => ((a as i64) >> (imm & 63)) as u64,
                _ => return Err(illegal),
            },
            0x1b => {
                let value = match funct {
                    0 => a.wrapping_add(imm) as u32,
                    1 if top == 0 => (a as u32) << (imm & 31),
                    5 if top == 0 => (a as u32) >> (imm & 31),
                    5 if top == 0x20 => ((a as i32) >> (imm & 31)) as u32,
                    _ => return Err(illegal),
                };
                value as i32 as u64
            }
            0x33 | 0x3b => {
                let word = opcode == 0x3b;
                let shift = b & if word { 31 } else { 63 };
                let a = if word { a as i32 as u64 } else { a };
                let b = if word { b as i32 as u64 } else { b };
                let value = match (top, funct) {
                    (0, 0) => a.wrapping_add(b),
                    (0x20, 0) => a.wrapping_sub(b),
                    (0, 1) => a << shift,
                    (0, 5) => {
                        if word {
                            (a as u32 as u64) >> shift
                        } else {
                            a >> shift
                        }
                    }
                    (0x20, 5) => ((a as i64) >> shift) as u64,
                    (0, 2) if !word => u64::from((a as i64) < b as i64),
                    (0, 3) if !word => u64::from(a < b),
                    (0, 4) if !word => a ^ b,
                    (0, 6) if !word => a | b,
                    (0, 7) if !word => a & b,
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
                self.instructions.fill(None);
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
