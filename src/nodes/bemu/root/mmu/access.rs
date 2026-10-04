use crate::root::platform::{Port, DRAM_BASE};
use rvsim::{
    bus::{Bus, Width},
    mmu::{Mmu, TranslationContext},
    Access,
};

pub(crate) struct GuestAccess<'a> {
    pub(crate) platform: Port<'a>,
    pub(crate) mmu: &'a Mmu,
    pub(crate) translation: TranslationContext<'a>,
}

impl GuestAccess<'_> {
    pub(crate) fn read_buffer(&mut self, mut address: u64, mut output: &mut [u8]) {
        while !output.is_empty() {
            let width = if address % 8 == 0 && output.len() >= 8 {
                Width::Double
            } else {
                Width::Byte
            };
            let physical = self
                .mmu
                .translate(&mut self.platform, &self.translation, address, width, Access::Load)
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
            let physical = self
                .mmu
                .translate(&mut self.platform, &self.translation, address, width, Access::Store)
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
