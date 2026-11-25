use crate::riscv::{mmu::Satp, pmp::Pmp, Privilege};

pub const SIE: u64 = 1 << 1;
pub const MIE: u64 = 1 << 3;
pub const SPIE: u64 = 1 << 5;
pub const MPIE: u64 = 1 << 7;
pub const SPP: u64 = 1 << 8;
pub const MPP: u64 = 3 << 11;
pub const FS: u64 = 3 << 13;
pub const MPRV: u64 = 1 << 17;
pub const TVM: u64 = 1 << 20;
pub const TW: u64 = 1 << 21;
pub const TSR: u64 = 1 << 22;
// Interrupt 16 is local: a DMA page fault of the hart's accelerator.
pub const DMA_FAULT_INTERRUPT: u64 = 1 << 16;
pub const INTERRUPTS: u64 = 0xaaa | DMA_FAULT_INTERRUPT;
pub const S_INTERRUPTS: u64 = 0x222 | DMA_FAULT_INTERRUPT;
const SSTATUS: u64 = SIE | SPIE | SPP | FS | (3 << 18);
const MSTATUS: u64 = SSTATUS | MIE | MPIE | MPP | MPRV | TVM | TW | TSR;
pub const MISA: u64 =
    (2 << 62) | (1 << 0) | (1 << 2) | (1 << 3) | (1 << 5) | (1 << 8) | (1 << 12) | (1 << 18) | (1 << 20);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CsrError;

/// The DMA page fault the hart's accelerator is stalled on: the virtual
/// address, whether it is a store, and the satp of the instruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DmaFault {
    pub address: u64,
    pub store: bool,
    pub satp: u64,
}

#[derive(Clone, Default)]
pub struct Csrs {
    pub status: u64,
    pub edeleg: u64,
    pub ideleg: u64,
    pub ie: u64,
    pub software_pending: u64,
    pub tvec: [u64; 2],
    pub scratch: [u64; 2],
    pub epc: [u64; 2],
    pub cause: [u64; 2],
    pub tval: [u64; 2],
    pub mcounteren: u64,
    pub scounteren: u64,
    pub inhibit: u64,
    pub cycle: u64,
    pub instret: u64,
    pub instret_written: bool,
    pub fcsr: u8,
    pub satp: Satp,
    pub pmp: Pmp,
    /// Pending as interrupt 16 until the hart resolves it through accfault.
    pub dma_fault: Option<DmaFault>,
}

impl Csrs {
    pub fn read(
        &self,
        address: u16,
        mode: Privilege,
        hart_id: u64,
        time: u64,
        interrupts: u64,
    ) -> Result<u64, CsrError> {
        self.check(address, mode)?;
        let sd = if self.status & FS == FS { 1 << 63 } else { 0 };
        let pending = self.software_pending | (interrupts & INTERRUPTS) | self.dma_pending();
        Ok(match address {
            0x001 => u64::from(self.fcsr & 31),
            0x002 => u64::from(self.fcsr >> 5),
            0x003 => self.fcsr.into(),
            0x100 => (self.status & SSTATUS) | (2 << 32) | sd,
            0x104 => self.ie & self.ideleg,
            0x105 => self.tvec[0],
            0x106 => self.scounteren,
            0x10a | 0x30a => 0, // no optional environment extensions
            0x140 => self.scratch[0],
            0x141 => self.epc[0],
            0x142 => self.cause[0],
            0x143 => self.tval[0],
            0x144 => pending & self.ideleg,
            0x180 => self.satp.read(),
            // accfault: bit 63 stalled, bit 62 a store, bits 61:0 the virtual address.
            0x5c0 => self.dma_fault.map_or(0, |fault| {
                1 << 63 | u64::from(fault.store) << 62 | fault.address & ((1 << 62) - 1)
            }),
            // accsatp: the satp the stalled instruction was issued under.
            0xdc0 => self.dma_fault.map_or(0, |fault| fault.satp),
            0x300 => self.status | (2 << 32) | (2 << 34) | sd,
            0x301 => MISA,
            0x302 => self.edeleg,
            0x303 => self.ideleg,
            0x304 => self.ie,
            0x305 => self.tvec[1],
            0x306 => self.mcounteren,
            0x320 => self.inhibit,
            0x340 => self.scratch[1],
            0x341 => self.epc[1],
            0x342 => self.cause[1],
            0x343 => self.tval[1],
            0x344 => pending,
            0x3a0 | 0x3a2 => {
                let start = if address == 0x3a0 { 0 } else { 8 };
                (0..8).fold(0, |value, i| value | u64::from(self.pmp.config(start + i)) << (8 * i))
            }
            0x3b0..=0x3bf => self.pmp.address((address - 0x3b0) as usize),
            0xb00 | 0xc00 => self.cycle,
            0xc01 => time,
            0xb02 | 0xc02 => self.instret,
            0xf11..=0xf13 | 0xf15 => 0,
            0xf14 => hart_id,
            _ => return Err(CsrError),
        })
    }

    pub fn write(&mut self, address: u16, value: u64, mode: Privilege) -> Result<(), CsrError> {
        self.check(address, mode)?;
        if address >> 10 == 3 {
            return Err(CsrError);
        }
        match address {
            0x001 => self.fcsr = (self.fcsr & !31) | (value as u8 & 31),
            0x002 => self.fcsr = (self.fcsr & 31) | ((value as u8 & 7) << 5),
            0x003 => self.fcsr = value as u8,
            0x100 => self.status = (self.status & !SSTATUS) | (value & SSTATUS),
            0x104 => self.ie = (self.ie & !self.ideleg) | (value & self.ideleg),
            0x105 | 0x305 => {
                let i = usize::from(address == 0x305);
                self.tvec[i] = (value & !3) | u64::from(value & 3 == 1);
            }
            0x106 => self.scounteren = value & 7,
            0x10a | 0x30a => (),
            0x140 | 0x340 => self.scratch[usize::from(address == 0x340)] = value,
            0x141 | 0x341 => self.epc[usize::from(address == 0x341)] = value & !1,
            0x142 | 0x342 => self.cause[usize::from(address == 0x342)] = value,
            0x143 | 0x343 => self.tval[usize::from(address == 0x343)] = value,
            0x144 => {
                let mask = self.ideleg & 2;
                self.software_pending = (self.software_pending & !mask) | (value & mask);
            }
            0x180 => self.satp.write(value),
            // Retry (bit 0 set) or fail the stalled DMA. A retried instruction
            // runs again when the hart reaches it; a failed one halts the
            // accelerator, which the simulation ends on.
            0x5c0 => {
                let fault = self.dma_fault.take().expect("accfault written without a stalled DMA");
                assert!(
                    value & 1 != 0,
                    "accelerator DMA {} fault at 0x{:x} failed by its owner",
                    if fault.store { "store" } else { "load" },
                    fault.address
                );
            }
            0x300 => {
                self.status = value & MSTATUS;
                if self.status & MPP == 2 << 11 {
                    self.status &= !MPP;
                }
            }
            0x301 => (),                           // RV64GC is fixed for this hart implementation.
            0x302 => self.edeleg = value & 0xb3ff, // no M-mode ECALL delegation
            0x303 => self.ideleg = value & S_INTERRUPTS,
            0x304 => self.ie = value & INTERRUPTS,
            0x306 => self.mcounteren = value & 7,
            0x320 => self.inhibit = value & 5,
            // A stalled DMA is the sole source of pending interrupt 16.
            0x344 => self.software_pending = value & (S_INTERRUPTS & !DMA_FAULT_INTERRUPT),
            0x3a0 | 0x3a2 => {
                let start = if address == 0x3a0 { 0 } else { 8 };
                for i in 0..8 {
                    self.pmp.set_config(start + i, (value >> (8 * i)) as u8);
                }
            }
            0x3b0..=0x3bf => self.pmp.set_address((address - 0x3b0) as usize, value),
            0xb00 => self.cycle = value,
            0xb02 => {
                self.instret = value;
                self.instret_written = true;
            }
            _ => return Err(CsrError),
        }
        if (1..=3).contains(&address) {
            self.status |= FS;
        }
        Ok(())
    }

    pub fn dma_pending(&self) -> u64 {
        if self.dma_fault.is_some() {
            DMA_FAULT_INTERRUPT
        } else {
            0
        }
    }

    fn check(&self, address: u16, mode: Privilege) -> Result<(), CsrError> {
        if address > 0xfff || (address >> 8) & 3 > mode as u16 {
            return Err(CsrError);
        }
        if (1..=3).contains(&address) && self.status & FS == 0 {
            return Err(CsrError);
        }
        if address == 0x180 && mode == Privilege::Supervisor && self.status & TVM != 0 {
            return Err(CsrError);
        }
        if (0xc00..=0xc02).contains(&address) && mode != Privilege::Machine {
            let bit = 1 << (address - 0xc00);
            if self.mcounteren & bit == 0 || (mode == Privilege::User && self.scounteren & bit == 0) {
                return Err(CsrError);
            }
        }
        Ok(())
    }
}
