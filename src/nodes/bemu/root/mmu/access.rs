use crate::root::platform::{Port, DRAM_BASE};
use rvsim::{
    bus::{Bus, Width},
    mmu::{Mmu, TranslationContext},
    Access, Trap,
};

pub(crate) struct GuestAccess<'a> {
    pub(crate) platform: Port<'a>,
    pub(crate) mmu: &'a Mmu,
    pub(crate) translation: TranslationContext<'a>,
    checked: Vec<(u64, u64, usize, Access)>,
}

impl<'a> GuestAccess<'a> {
    pub(crate) fn new(platform: Port<'a>, mmu: &'a Mmu, translation: TranslationContext<'a>) -> Self {
        GuestAccess { platform, mmu, translation, checked: Vec::new() }
    }

    pub(crate) fn check_range(&mut self, mut address: u64, mut bytes: usize, access: Access) -> Result<(), Trap> {
        address.checked_add(bytes as u64).ok_or_else(|| access.fault(address))?;
        while bytes > 0 {
            let count = bytes.min(4096 - (address as usize & 4095));
            let physical = self.mmu.translate(&mut self.platform, &self.translation, address, Width::Byte, access)?;
            let end = physical.checked_add(count as u64).ok_or_else(|| access.fault(address))?;
            if physical < DRAM_BASE || end > DRAM_BASE + self.platform.memory.len() as u64 {
                return Err(access.fault(address));
            }
            if !self.translation.pmp.allows_range(physical, count, access, self.translation.privilege) {
                for offset in 0..count {
                    if !self.translation.pmp.allows(physical + offset as u64, Width::Byte, access, self.translation.privilege) {
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
            let grant = self.checked.iter().find(|(v, _, n, a)| *a == access && address >= *v
                && address - *v + width as u64 <= *n as u64).expect("DMA access outside checked footprint");
            return Ok(grant.1 + address - grant.0);
        }
        self.mmu.translate(&mut self.platform, &self.translation, address, width, access)
    }

    pub(crate) fn read_buffer(&mut self, mut address: u64, mut output: &mut [u8]) {
        while !output.is_empty() {
            let width = if address % 8 == 0 && output.len() >= 8 {
                Width::Double
            } else {
                Width::Byte
            };
            let physical = self.translate(address, width, Access::Load)
                .unwrap_or_else(|trap| panic!("DMA load at 0x{address:x}: {trap:?}"));
            let bytes = output.len().min(4096 - (address as usize & 4095));
            if self.translation.privilege == rvsim::Privilege::User
                && physical >= DRAM_BASE
                && self
                    .translation
                    .pmp
                    .allows_range(physical, bytes, Access::Load, rvsim::Privilege::User)
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

    pub(crate) fn write_buffer(&mut self, mut address: u64, mut input: &[u8]) {
        while !input.is_empty() {
            let width = if address % 8 == 0 && input.len() >= 8 {
                Width::Double
            } else {
                Width::Byte
            };
            let physical = self.translate(address, width, Access::Store)
                .unwrap_or_else(|trap| panic!("DMA store at 0x{address:x}: {trap:?}"));
            let bytes = input.len().min(4096 - (address as usize & 4095));
            if self.translation.privilege == rvsim::Privilege::User
                && physical >= DRAM_BASE
                && self
                    .translation
                    .pmp
                    .allows_range(physical, bytes, Access::Store, rvsim::Privilege::User)
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

    pub(crate) fn read(&mut self, address: u64) -> u8 {
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
