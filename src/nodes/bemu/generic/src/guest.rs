use crate::ddr::DRAM_BASE;
use crate::hlink::Port;
use crate::riscv::{
    bus::{Bus, Width},
    mmu::{Mmu, TranslationContext},
    Access, Privilege, Trap,
};

pub trait ThreadMemory: Send + Sync {
    /// Whether an access of the thread that starts at `address` can be in this memory, and the address that answer
    /// holds up to: a DMA asks once for the rows it moves inside one answer.
    fn span(&self, thread: usize, address: u64, write: bool) -> (bool, u64);
    fn contains(&self, thread: usize, address: u64, bytes: usize, write: bool) -> bool;
    fn transfer_cycles(&self, thread: usize, spans: &[(u64, usize)], write: bool) -> Option<u64>;
    fn start(&self, thread: usize, task: u64);
    fn control(&self, operation: u32, field: u64, value: u64) -> u64;
    fn read(&self, thread: usize, address: u64, output: &mut [u8]) -> bool;
    fn write(&self, thread: usize, address: u64, input: &[u8]) -> bool;
}

/// A DMA as the hart's memory path times it: its source, the bank rows and the DDR ranges it
/// moves, and the cycles of the rows that stay in hart-local storage.
pub struct Transfer {
    pub source: usize,
    pub rows: u64,
    pub ranges: Vec<(u64, usize)>,
    pub local: Option<u64>,
    pub write: bool,
}

impl Transfer {
    /// The cycles of the DMA when it starts at `start`, reserved on the hart's memory path.
    pub fn cycles(&self, hart: &mut crate::timing::HartTime, start: u64) -> u64 {
        hart.transfer(
            self.source,
            start,
            self.rows,
            self.ranges.iter().copied(),
            None,
            self.write,
        ) + self.local.unwrap_or(0)
    }
}

pub struct GuestAccess<'a> {
    pub owner: &'a crate::tlink::TLink,
    pub platform: Port<'a>,
    pub mmu: &'a Mmu,
    pub translation: TranslationContext<'a>,
    checked: Vec<(u64, u64, usize, Access)>,
    /// The checked range the last access fell in: a DMA walks its footprint in order.
    last_checked: usize,
    /// The last answer of the hart-local memory for a load and for a store: from, up to, inside.
    local_span: [(u64, u64, bool); 2],
    local_spans: Vec<(u64, usize)>,
    pub local: Option<(&'a dyn ThreadMemory, usize)>,
    source: usize,
}

impl<'a> GuestAccess<'a> {
    pub fn stage_cycles(&self, start: u64, phases: &crate::timing::ServicePhases) -> u64 {
        assert!(
            self.local.is_none(),
            "Device service memory does not use hart-local storage"
        );
        let machine_unlocked_off = self.translation.effective_privilege(Access::Load) == Privilege::Machine
            && (0..16).all(|i| self.translation.pmp.config(i) & 0x98 == 0);
        if !machine_unlocked_off {
            // Other permission paths retain their unqualified conservative timing. Functional
            // translation and PMP authorization have already checked the complete payload.
            let rows = self
                .checked
                .iter()
                .map(|(_, _, bytes, _)| (*bytes as u64).div_ceil(16))
                .sum();
            return phases.stream * 4 + self.transfer_cycles(start, rows, false);
        }
        self.owner.transfers.lock().unwrap().stage(
            self.source,
            start,
            self.checked.iter().map(|(_, physical, bytes, _)| (*physical, *bytes)),
            phases,
        )
    }

    /// The DMA of the checked footprint as the hart's memory path times it.
    pub fn transfer(&self, rows: u64, write: bool) -> Transfer {
        let local = self
            .local
            .and_then(|(memory, thread)| memory.transfer_cycles(thread, &self.local_spans, write));
        let ddr_rows = if local.is_some() {
            self.checked
                .iter()
                .map(|(_, _, bytes, _)| (*bytes as u64).div_ceil(16))
                .sum()
        } else {
            rows
        };
        Transfer {
            source: self.source,
            rows: ddr_rows,
            ranges: self
                .checked
                .iter()
                .map(|(_, physical, bytes, _)| (*physical, *bytes))
                .collect(),
            local,
            write,
        }
    }

    pub fn transfer_cycles(&self, start: u64, rows: u64, write: bool) -> u64 {
        self.transfer(rows, write)
            .cycles(&mut self.owner.transfers.lock().unwrap(), start)
    }

    pub fn new(
        platform: Port<'a>,
        mmu: &'a Mmu,
        translation: TranslationContext<'a>,
        owner: &'a crate::tlink::TLink,
        source: usize,
    ) -> Self {
        GuestAccess {
            owner,
            platform,
            mmu,
            translation,
            checked: Vec::new(),
            last_checked: 0,
            local_span: [(0, 0, false); 2],
            local_spans: Vec::new(),
            local: None,
            source,
        }
    }

    /// Device memory as a master outside the hart's translation reads it.
    pub fn read_physical(&mut self, address: u64, output: &mut [u8]) {
        self.platform
            .memory
            .read_buffer(address, output)
            .expect("physical load");
    }

    /// Device memory as a master outside the hart's translation writes it.
    pub fn write_physical(&mut self, address: u64, input: &[u8]) {
        self.platform
            .memory
            .write_buffer(address, input)
            .expect("physical store");
    }

    /// The hart-local memory, where an access at `address` can be in it.
    fn local_at(&mut self, address: u64, write: bool) -> Option<(&'a dyn ThreadMemory, usize)> {
        let (memory, thread) = self.local?;
        let span = &mut self.local_span[usize::from(write)];
        if address < span.0 || address >= span.1 {
            let (inside, end) = memory.span(thread, address, write);
            *span = (address, end, inside);
        }
        span.2.then_some((memory, thread))
    }

    pub fn check_range(&mut self, mut address: u64, mut bytes: usize, access: Access) -> Result<(), Trap> {
        address.checked_add(bytes as u64).ok_or_else(|| access.fault(address))?;
        if self
            .local_at(address, access == Access::Store)
            .is_some_and(|(memory, thread)| memory.contains(thread, address, bytes, access == Access::Store))
        {
            self.local_spans.push((address, bytes));
            return Ok(());
        }
        let privilege = self.translation.effective_privilege(access);
        while bytes > 0 {
            let count = bytes.min(4096 - (address as usize & 4095));
            let physical = self
                .mmu
                .translate(&mut self.platform, &self.translation, address, Width::Byte, access)?;
            let end = physical
                .checked_add(count as u64)
                .ok_or_else(|| access.fault(address))?;
            if physical < DRAM_BASE || end > DRAM_BASE + self.platform.memory.len() as u64 {
                return Err(access.fault(address));
            }
            if !self.translation.pmp.allows_range(physical, count, access, privilege) {
                for offset in 0..count {
                    if !self
                        .translation
                        .pmp
                        .allows(physical + offset as u64, Width::Byte, access, privilege)
                    {
                        return Err(access.fault(address + offset as u64));
                    }
                }
                return Err(access.fault(address));
            }
            self.checked.push((address, physical, count, access));
            address += count as u64;
            bytes -= count;
        }
        Ok(())
    }

    fn translate(&mut self, address: u64, width: Width, access: Access) -> Result<u64, Trap> {
        if !self.checked.is_empty() {
            let ranges = self.checked.len();
            let index = (0..ranges)
                .map(|step| (self.last_checked + step) % ranges)
                .find(|&index| {
                    let (v, _, n, a) = self.checked[index];
                    a == access && address >= v && address - v + width as u64 <= n as u64
                })
                .expect("DMA access outside checked footprint");
            self.last_checked = index;
            let grant = self.checked[index];
            return Ok(grant.1 + address - grant.0);
        }
        self.mmu
            .translate(&mut self.platform, &self.translation, address, width, access)
    }

    pub fn read_buffer(&mut self, mut address: u64, mut output: &mut [u8]) {
        if self
            .local_at(address, false)
            .is_some_and(|(memory, thread)| memory.read(thread, address, output))
        {
            return;
        }
        while !output.is_empty() {
            let width = if address % 8 == 0 && output.len() >= 8 {
                Width::Double
            } else {
                Width::Byte
            };
            let physical = self
                .translate(address, width, Access::Load)
                .unwrap_or_else(|trap| panic!("DMA load at 0x{address:x}: {trap:?}"));
            let bytes = output.len().min(4096 - (address as usize & 4095));
            if physical >= DRAM_BASE
                && self.translation.pmp.allows_range(
                    physical,
                    bytes,
                    Access::Load,
                    self.translation.effective_privilege(Access::Load),
                )
            {
                self.platform
                    .memory
                    .read_buffer(physical, &mut output[..bytes])
                    .expect("DMA physical load");
                output = &mut output[bytes..];
                address += bytes as u64;
                continue;
            }
            let bytes = self
                .platform
                .read(physical, width)
                .expect("DMA physical load")
                .to_le_bytes();
            let size = width as usize;
            output[..size].copy_from_slice(&bytes[..size]);
            output = &mut output[size..];
            address += size as u64;
        }
    }

    pub fn write_buffer(&mut self, mut address: u64, mut input: &[u8]) {
        if self
            .local_at(address, true)
            .is_some_and(|(memory, thread)| memory.write(thread, address, input))
        {
            return;
        }
        while !input.is_empty() {
            let width = if address % 8 == 0 && input.len() >= 8 {
                Width::Double
            } else {
                Width::Byte
            };
            let physical = self
                .translate(address, width, Access::Store)
                .unwrap_or_else(|trap| panic!("DMA store at 0x{address:x}: {trap:?}"));
            let bytes = input.len().min(4096 - (address as usize & 4095));
            if physical >= DRAM_BASE
                && self.translation.pmp.allows_range(
                    physical,
                    bytes,
                    Access::Store,
                    self.translation.effective_privilege(Access::Store),
                )
            {
                self.platform
                    .memory
                    .write_buffer(physical, &input[..bytes])
                    .expect("DMA physical store");
                input = &input[bytes..];
                address += bytes as u64;
                continue;
            }
            let size = width as usize;
            let mut bytes = [0; 8];
            bytes[..size].copy_from_slice(&input[..size]);
            self.platform
                .write(physical, width, u64::from_le_bytes(bytes))
                .expect("DMA physical store");
            input = &input[size..];
            address += size as u64;
        }
    }

    pub fn read(&mut self, address: u64) -> u8 {
        let mut local = [0];
        if self
            .local
            .is_some_and(|(memory, thread)| memory.read(thread, address, &mut local))
        {
            return local[0];
        }
        let physical = self
            .mmu
            .translate(
                &mut self.platform,
                &self.translation,
                address,
                Width::Byte,
                Access::Load,
            )
            .unwrap_or_else(|trap| panic!("DMA load at 0x{address:x}: {trap:?}"));
        self.platform.read(physical, Width::Byte).expect("DMA physical load") as u8
    }
}
