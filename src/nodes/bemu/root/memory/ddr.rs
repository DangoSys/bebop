use crate::root::platform::DRAM_BASE;
use rvsim::bus::{BusError, Width};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock, RwLock},
};

const PAGE_BYTES: usize = 64 * 1024;

struct Page {
    data: Vec<u8>,
    reservations: HashMap<u64, (u64, Width)>,
}

impl Page {
    fn new() -> Self {
        Self {
            data: vec![0; PAGE_BYTES],
            reservations: HashMap::new(),
        }
    }

    fn invalidate(&mut self, address: u64, bytes: usize) {
        self.reservations
            .retain(|_, (start, width)| *start + *width as u64 <= address || address + bytes as u64 <= *start);
    }
}

pub(crate) struct Ddr {
    bytes: usize,
    pages: Vec<OnceLock<RwLock<Page>>>,
    reservations: Mutex<HashMap<u64, (u64, Width)>>,
}

impl Ddr {
    pub(crate) fn new(bytes: usize) -> Self {
        Self {
            bytes,
            pages: (0..bytes.div_ceil(PAGE_BYTES)).map(|_| OnceLock::new()).collect(),
            reservations: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.bytes
    }

    fn offset(&self, address: u64, bytes: usize) -> Result<usize, BusError> {
        let offset = address.checked_sub(DRAM_BASE).ok_or(BusError)? as usize;
        if offset.checked_add(bytes).ok_or(BusError)? > self.bytes {
            return Err(BusError);
        }
        Ok(offset)
    }

    pub(crate) fn read_buffer(&self, address: u64, mut output: &mut [u8]) -> Result<(), BusError> {
        let mut offset = self.offset(address, output.len())?;
        while !output.is_empty() {
            let within = offset % PAGE_BYTES;
            let bytes = output.len().min(PAGE_BYTES - within);
            if let Some(page) = self.pages[offset / PAGE_BYTES].get() {
                let page = page.read().expect("DDR page poisoned");
                output[..bytes].copy_from_slice(&page.data[within..within + bytes]);
            } else {
                output[..bytes].fill(0);
            }
            offset += bytes;
            output = &mut output[bytes..];
        }
        Ok(())
    }

    pub(crate) fn write_buffer(&self, address: u64, mut input: &[u8]) -> Result<(), BusError> {
        let mut offset = self.offset(address, input.len())?;
        while !input.is_empty() {
            let within = offset % PAGE_BYTES;
            let bytes = input.len().min(PAGE_BYTES - within);
            let mut page = self.pages[offset / PAGE_BYTES]
                .get_or_init(|| RwLock::new(Page::new()))
                .write()
                .expect("DDR page poisoned");
            page.data[within..within + bytes].copy_from_slice(&input[..bytes]);
            page.invalidate(DRAM_BASE + offset as u64, bytes);
            offset += bytes;
            input = &input[bytes..];
        }
        Ok(())
    }

    pub(crate) fn fill(&self, address: u64, bytes: usize, value: u8) -> Result<(), BusError> {
        let mut offset = self.offset(address, bytes)?;
        let end = offset + bytes;
        while offset < end {
            let within = offset % PAGE_BYTES;
            let bytes = (end - offset).min(PAGE_BYTES - within);
            if value != 0 || self.pages[offset / PAGE_BYTES].get().is_some() {
                let mut page = self.pages[offset / PAGE_BYTES]
                    .get_or_init(|| RwLock::new(Page::new()))
                    .write()
                    .expect("DDR page poisoned");
                page.data[within..within + bytes].fill(value);
                page.invalidate(DRAM_BASE + offset as u64, bytes);
            }
            offset += bytes;
        }
        Ok(())
    }

    pub(crate) fn read(&self, address: u64, width: Width) -> Result<u64, BusError> {
        let offset = self.offset(address, width as usize)?;
        let Some(page) = self.pages[offset / PAGE_BYTES].get() else {
            return Ok(0);
        };
        let page = page.read().expect("DDR page poisoned");
        let within = offset % PAGE_BYTES;
        Ok(match width {
            Width::Byte => u64::from(page.data[within]),
            Width::Half => u64::from(u16::from_le_bytes(page.data[within..within + 2].try_into().unwrap())),
            Width::Word => u64::from(u32::from_le_bytes(page.data[within..within + 4].try_into().unwrap())),
            Width::Double => u64::from_le_bytes(page.data[within..within + 8].try_into().unwrap()),
        })
    }

    pub(crate) fn write(&self, address: u64, width: Width, value: u64) -> Result<(), BusError> {
        let offset = self.offset(address, width as usize)?;
        let mut page = self.pages[offset / PAGE_BYTES]
            .get_or_init(|| RwLock::new(Page::new()))
            .write()
            .expect("DDR page poisoned");
        let within = offset % PAGE_BYTES;
        match width {
            Width::Byte => page.data[within] = value as u8,
            Width::Half => page.data[within..within + 2].copy_from_slice(&(value as u16).to_le_bytes()),
            Width::Word => page.data[within..within + 4].copy_from_slice(&(value as u32).to_le_bytes()),
            Width::Double => page.data[within..within + 8].copy_from_slice(&value.to_le_bytes()),
        }
        page.invalidate(address, width as usize);
        Ok(())
    }

    pub(crate) fn compare_exchange(
        &self,
        address: u64,
        width: Width,
        expected: u64,
        value: u64,
    ) -> Result<u64, BusError> {
        let offset = self.offset(address, width as usize)?;
        if address % width as u64 != 0 {
            return Err(BusError);
        }
        let within = offset % PAGE_BYTES;
        let mut page = self.pages[offset / PAGE_BYTES]
            .get_or_init(|| RwLock::new(Page::new()))
            .write()
            .expect("DDR page poisoned");
        let mut bytes = [0; 8];
        bytes[..width as usize].copy_from_slice(&page.data[within..within + width as usize]);
        let old = u64::from_le_bytes(bytes);
        if old == expected {
            page.data[within..within + width as usize].copy_from_slice(&value.to_le_bytes()[..width as usize]);
            page.invalidate(address, width as usize);
        }
        Ok(old)
    }

    pub(crate) fn load_reserved(&self, hart: u64, address: u64, width: Width) -> Result<u64, BusError> {
        let offset = self.offset(address, width as usize)?;
        if address % width as u64 != 0 {
            return Err(BusError);
        }
        let mut reservations = self.reservations.lock().expect("DDR reservations poisoned");
        if let Some((previous, _)) = reservations.remove(&hart) {
            self.pages[(previous - DRAM_BASE) as usize / PAGE_BYTES]
                .get()
                .unwrap()
                .write()
                .expect("DDR page poisoned")
                .reservations
                .remove(&hart);
        }
        let within = offset % PAGE_BYTES;
        let mut page = self.pages[offset / PAGE_BYTES]
            .get_or_init(|| RwLock::new(Page::new()))
            .write()
            .expect("DDR page poisoned");
        let mut bytes = [0; 8];
        bytes[..width as usize].copy_from_slice(&page.data[within..within + width as usize]);
        page.reservations.insert(hart, (address, width));
        reservations.insert(hart, (address, width));
        Ok(u64::from_le_bytes(bytes))
    }

    pub(crate) fn store_conditional(
        &self,
        hart: u64,
        address: u64,
        width: Width,
        value: u64,
    ) -> Result<bool, BusError> {
        let offset = self.offset(address, width as usize)?;
        if address % width as u64 != 0 {
            return Err(BusError);
        }
        let mut reservations = self.reservations.lock().expect("DDR reservations poisoned");
        let previous = reservations.remove(&hart);
        let Some((previous_address, previous_width)) = previous else {
            return Ok(false);
        };
        let mut page = self.pages[(previous_address - DRAM_BASE) as usize / PAGE_BYTES]
            .get()
            .unwrap()
            .write()
            .expect("DDR page poisoned");
        let success = page.reservations.remove(&hart) == Some((address, width))
            && previous_address == address
            && previous_width == width;
        if success {
            let within = offset % PAGE_BYTES;
            page.data[within..within + width as usize].copy_from_slice(&value.to_le_bytes()[..width as usize]);
            page.invalidate(address, width as usize);
        }
        Ok(success)
    }

    pub(crate) fn clear_reservations(&self) {
        let mut reservations = self.reservations.lock().expect("DDR reservations poisoned");
        for (hart, (address, _)) in reservations.drain() {
            self.pages[(address - DRAM_BASE) as usize / PAGE_BYTES]
                .get()
                .unwrap()
                .write()
                .expect("DDR page poisoned")
                .reservations
                .remove(&hart);
        }
    }
}

impl memory::Memory for Ddr {
    fn len(&self) -> usize {
        self.bytes
    }

    fn read_buffer(&self, offset: usize, output: &mut [u8]) {
        Ddr::read_buffer(self, DRAM_BASE + offset as u64, output).expect("DDR read range");
    }

    fn write_buffer(&self, offset: usize, input: &[u8]) {
        Ddr::write_buffer(self, DRAM_BASE + offset as u64, input).expect("DDR write range");
    }

    fn fill(&self, offset: usize, bytes: usize, value: u8) {
        Ddr::fill(self, DRAM_BASE + offset as u64, bytes, value).expect("DDR fill range");
    }
}
