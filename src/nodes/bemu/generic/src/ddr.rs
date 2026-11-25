use crate::riscv::bus::{BusError, Width};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Mutex, OnceLock, RwLock,
    },
};

pub const DRAM_BASE: u64 = 0x8000_0000;
pub const DTB_ADDRESS: u64 = 0x9000_0000;
pub const INITRD_ADDRESS: u64 = 0x9100_0000;

const PAGE_BYTES: usize = 64 * 1024;

/// A hart's reservation: the address and width it loaded, and the value it found there.
type Reservation = (u64, Width, u64);

struct Page {
    data: Vec<AtomicU64>,
    /// The reservations on the page, by hart. Whoever writes more than one word holds this exclusively, whoever
    /// reads more than one holds it shared: a read sees such a write whole.
    writes: RwLock<HashMap<u64, Reservation>>,
    /// How many reservations the page holds: a store inside one word looks here and leaves `writes` alone.
    reserved: AtomicUsize,
}

impl Page {
    fn new() -> Self {
        Self {
            data: (0..PAGE_BYTES / 8).map(|_| AtomicU64::new(0)).collect(),
            writes: RwLock::new(HashMap::new()),
            reserved: AtomicUsize::new(0),
        }
    }

    fn invalidate(&self, reservations: &mut HashMap<u64, Reservation>, address: u64, bytes: usize) {
        reservations
            .retain(|_, (start, width, _)| *start + *width as u64 <= address || address + bytes as u64 <= *start);
        self.reserved.store(reservations.len(), Ordering::SeqCst);
    }

    fn release(&self, hart: u64) {
        let mut reservations = self.writes.write().expect("DDR page poisoned");
        reservations.remove(&hart);
        self.reserved.store(reservations.len(), Ordering::SeqCst);
    }

    fn read(&self, within: usize, bytes: usize) -> u64 {
        let shift = (within % 8) * 8;
        let mut value = self.data[within / 8].load(Ordering::Acquire).to_le() >> shift;
        if shift + bytes * 8 > 64 {
            value |= self.data[within / 8 + 1].load(Ordering::Acquire).to_le() << (64 - shift);
        }
        value & (u64::MAX >> ((8 - bytes) * 8))
    }

    fn load(&self, mut within: usize, mut output: &mut [u8]) {
        let head = output.len().min((8 - within % 8) % 8);
        let word = self.data[within / 8].load(Ordering::Acquire).to_ne_bytes();
        output[..head].copy_from_slice(&word[within % 8..within % 8 + head]);
        within += head;
        output = &mut output[head..];
        let last = (within + output.len()) / 8;
        let mut words = output.chunks_exact_mut(8);
        for (bytes, word) in (&mut words).zip(&self.data[within / 8..]) {
            bytes.copy_from_slice(&word.load(Ordering::Acquire).to_ne_bytes());
        }
        let tail = words.into_remainder();
        if !tail.is_empty() {
            let word = self.data[last].load(Ordering::Acquire).to_ne_bytes();
            tail.copy_from_slice(&word[..tail.len()]);
        }
    }

    // A whole word is one store, part of a word one atomic update of its bytes: a store that holds `writes` and
    // one that does not never lose each other's bytes.
    fn store(&self, mut within: usize, mut input: &[u8]) {
        while !input.is_empty() {
            let count = input.len().min(8 - within % 8);
            let word = &self.data[within / 8];
            if count == 8 {
                word.store(u64::from_ne_bytes(input[..8].try_into().unwrap()), Ordering::Release);
            } else {
                let mut bytes = [0; 8];
                bytes[..count].copy_from_slice(&input[..count]);
                let shift = (within % 8) * 8;
                let mask = (u64::MAX >> ((8 - count) * 8)) << shift;
                let value = u64::from_le_bytes(bytes) << shift;
                word.fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                    Some(((old.to_le() & !mask) | value).to_le())
                })
                .unwrap();
            }
            within += count;
            input = &input[count..];
        }
    }

    /// Replaces the `bytes` at `within`, inside one word, by `value` if they are `expected`. Returns what they were.
    fn exchange(&self, within: usize, bytes: usize, expected: u64, value: u64) -> u64 {
        let shift = (within % 8) * 8;
        let field = u64::MAX >> ((8 - bytes) * 8);
        let word = &self.data[within / 8];
        let mut old = word.load(Ordering::Acquire);
        loop {
            let current = (old.to_le() >> shift) & field;
            if current != expected {
                return current;
            }
            let new = (old.to_le() & !(field << shift)) | ((value & field) << shift);
            match word.compare_exchange_weak(old, new.to_le(), Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return current,
                Err(now) => old = now,
            }
        }
    }
}

pub struct Ddr {
    bytes: usize,
    pages: Vec<OnceLock<Page>>,
    /// The page each hart holds its reservation on.
    reservations: Mutex<HashMap<u64, u64>>,
}

impl Ddr {
    pub fn new(bytes: usize) -> Self {
        Self {
            bytes,
            pages: (0..bytes.div_ceil(PAGE_BYTES)).map(|_| OnceLock::new()).collect(),
            reservations: Mutex::new(HashMap::new()),
        }
    }

    pub fn len(&self) -> usize {
        self.bytes
    }

    fn offset(&self, address: u64, bytes: usize) -> Result<usize, BusError> {
        let offset = address.checked_sub(DRAM_BASE).ok_or(BusError)? as usize;
        if offset.checked_add(bytes).ok_or(BusError)? > self.bytes {
            return Err(BusError);
        }
        Ok(offset)
    }

    pub fn read_buffer(&self, address: u64, mut output: &mut [u8]) -> Result<(), BusError> {
        let mut offset = self.offset(address, output.len())?;
        while !output.is_empty() {
            let within = offset % PAGE_BYTES;
            let bytes = output.len().min(PAGE_BYTES - within);
            if let Some(page) = self.pages[offset / PAGE_BYTES].get() {
                let _read = page.writes.read().expect("DDR page poisoned");
                page.load(within, &mut output[..bytes]);
            } else {
                output[..bytes].fill(0);
            }
            offset += bytes;
            output = &mut output[bytes..];
        }
        Ok(())
    }

    pub fn write_buffer(&self, address: u64, mut input: &[u8]) -> Result<(), BusError> {
        let mut offset = self.offset(address, input.len())?;
        while !input.is_empty() {
            let within = offset % PAGE_BYTES;
            let bytes = input.len().min(PAGE_BYTES - within);
            let page = self.pages[offset / PAGE_BYTES].get_or_init(Page::new);
            let mut reservations = page.writes.write().expect("DDR page poisoned");
            page.store(within, &input[..bytes]);
            page.invalidate(&mut reservations, DRAM_BASE + offset as u64, bytes);
            offset += bytes;
            input = &input[bytes..];
        }
        Ok(())
    }

    pub fn fill(&self, address: u64, bytes: usize, value: u8) -> Result<(), BusError> {
        let mut offset = self.offset(address, bytes)?;
        let end = offset + bytes;
        while offset < end {
            let within = offset % PAGE_BYTES;
            let bytes = (end - offset).min(PAGE_BYTES - within);
            if value != 0 || self.pages[offset / PAGE_BYTES].get().is_some() {
                let page = self.pages[offset / PAGE_BYTES].get_or_init(Page::new);
                let mut reservations = page.writes.write().expect("DDR page poisoned");
                let mut cursor = within;
                while cursor < within + bytes {
                    let count = (8 - cursor % 8).min(within + bytes - cursor);
                    page.store(cursor, &[value; 8][..count]);
                    cursor += count;
                }
                page.invalidate(&mut reservations, DRAM_BASE + offset as u64, bytes);
            }
            offset += bytes;
        }
        Ok(())
    }

    #[inline]
    pub fn read(&self, address: u64, width: Width) -> Result<u64, BusError> {
        let offset = self.offset(address, width as usize)?;
        let Some(page) = self.pages[offset / PAGE_BYTES].get() else {
            return Ok(0);
        };
        let within = offset % PAGE_BYTES;
        if within % 8 + width as usize > 8 {
            let _read = page.writes.read().expect("DDR page poisoned");
            return Ok(page.read(within, width as usize));
        }
        Ok(page.read(within, width as usize))
    }

    #[inline]
    pub fn write(&self, address: u64, width: Width, value: u64) -> Result<(), BusError> {
        let offset = self.offset(address, width as usize)?;
        let page = self.pages[offset / PAGE_BYTES].get_or_init(Page::new);
        let within = offset % PAGE_BYTES;
        let bytes = &value.to_le_bytes()[..width as usize];
        if within % 8 + width as usize > 8 {
            let mut reservations = page.writes.write().expect("DDR page poisoned");
            page.store(within, bytes);
            page.invalidate(&mut reservations, address, width as usize);
            return Ok(());
        }
        page.store(within, bytes);
        // A reservation taken while this store was under way is not seen here: its store-conditional finds
        // another value than it loaded, or the store happened before the load for all anyone can tell.
        if page.reserved.load(Ordering::SeqCst) != 0 {
            let mut reservations = page.writes.write().expect("DDR page poisoned");
            page.invalidate(&mut reservations, address, width as usize);
        }
        Ok(())
    }

    pub fn compare_exchange(&self, address: u64, width: Width, expected: u64, value: u64) -> Result<u64, BusError> {
        let offset = self.offset(address, width as usize)?;
        if address % width as u64 != 0 {
            return Err(BusError);
        }
        let within = offset % PAGE_BYTES;
        let page = self.pages[offset / PAGE_BYTES].get_or_init(Page::new);
        let mut reservations = page.writes.write().expect("DDR page poisoned");
        let old = page.exchange(within, width as usize, expected, value);
        if old == expected {
            page.invalidate(&mut reservations, address, width as usize);
        }
        Ok(old)
    }

    pub fn load_reserved(&self, hart: u64, address: u64, width: Width) -> Result<u64, BusError> {
        let offset = self.offset(address, width as usize)?;
        if address % width as u64 != 0 {
            return Err(BusError);
        }
        let mut reservations = self.reservations.lock().expect("DDR reservations poisoned");
        if let Some(previous) = reservations.remove(&hart) {
            self.pages[previous as usize].get().unwrap().release(hart);
        }
        let within = offset % PAGE_BYTES;
        let page = self.pages[offset / PAGE_BYTES].get_or_init(Page::new);
        let mut writes = page.writes.write().expect("DDR page poisoned");
        // Every store from here on sees the reservation; the value is loaded after that.
        page.reserved.store(writes.len() + 1, Ordering::SeqCst);
        let value = page.read(within, width as usize);
        writes.insert(hart, (address, width, value));
        reservations.insert(hart, (offset / PAGE_BYTES) as u64);
        Ok(value)
    }

    pub fn store_conditional(&self, hart: u64, address: u64, width: Width, value: u64) -> Result<bool, BusError> {
        let offset = self.offset(address, width as usize)?;
        if address % width as u64 != 0 {
            return Err(BusError);
        }
        let mut reservations = self.reservations.lock().expect("DDR reservations poisoned");
        let Some(previous) = reservations.remove(&hart) else {
            return Ok(false);
        };
        let page = self.pages[previous as usize].get().unwrap();
        let mut writes = page.writes.write().expect("DDR page poisoned");
        let reservation = writes.remove(&hart);
        page.reserved.store(writes.len(), Ordering::SeqCst);
        let Some((_, _, loaded)) = reservation.filter(|reserved| (reserved.0, reserved.1) == (address, width)) else {
            return Ok(false);
        };
        let success = page.exchange(offset % PAGE_BYTES, width as usize, loaded, value) == loaded;
        if success {
            page.invalidate(&mut writes, address, width as usize);
        }
        Ok(success)
    }

    pub fn clear_reservations(&self) {
        let mut reservations = self.reservations.lock().expect("DDR reservations poisoned");
        for (hart, page) in reservations.drain() {
            self.pages[page as usize].get().unwrap().release(hart);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ddr_byte_ranges_and_reservations() {
        let memory = Ddr::new(PAGE_BYTES * 2);
        assert_eq!(memory.read(DRAM_BASE, Width::Double).unwrap(), 0);
        let start = DRAM_BASE + PAGE_BYTES as u64 - 17;
        let bytes: Vec<u8> = (0..257).map(|i| (i * 37) as u8).collect();
        memory.write_buffer(start, &bytes).unwrap();
        let mut actual = vec![0; bytes.len()];
        memory.read_buffer(start, &mut actual).unwrap();
        assert_eq!(actual, bytes);
        for i in 0..bytes.len() - 8 {
            let address = start + i as u64;
            if address as usize % PAGE_BYTES + 8 <= PAGE_BYTES {
                assert_eq!(
                    memory.read(address, Width::Double).unwrap(),
                    u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap())
                );
            }
        }
        memory.fill(start + 3, 233, 0x5a).unwrap();
        memory.read_buffer(start, &mut actual).unwrap();
        assert_eq!(&actual[..3], &bytes[..3]);
        assert!(actual[3..236].iter().all(|&b| b == 0x5a));
        assert_eq!(&actual[236..], &bytes[236..]);

        let address = DRAM_BASE + 64;
        memory.write(address, Width::Double, 17).unwrap();
        assert_eq!(memory.load_reserved(0, address, Width::Double).unwrap(), 17);
        memory.write(address + 3, Width::Byte, 9).unwrap();
        assert!(!memory.store_conditional(0, address, Width::Double, 18).unwrap());
        memory.load_reserved(0, address, Width::Double).unwrap();
        memory.write(address + 16, Width::Word, 23).unwrap();
        assert!(memory.store_conditional(0, address, Width::Double, 18).unwrap());
        memory.load_reserved(0, address, Width::Double).unwrap();
        assert_eq!(memory.compare_exchange(address, Width::Word, 19, 20).unwrap(), 18);
        assert!(memory.store_conditional(0, address, Width::Double, 19).unwrap());
        memory.load_reserved(0, address, Width::Double).unwrap();
        assert_eq!(memory.compare_exchange(address, Width::Word, 19, 20).unwrap(), 19);
        assert!(!memory.store_conditional(0, address, Width::Double, 21).unwrap());
        memory.load_reserved(0, address, Width::Double).unwrap();
        memory.fill(address + 7, 2, 0).unwrap();
        assert!(!memory.store_conditional(0, address, Width::Double, 21).unwrap());
        memory.load_reserved(0, address, Width::Double).unwrap();
        memory.clear_reservations();
        assert!(!memory.store_conditional(0, address, Width::Double, 21).unwrap());
    }

    #[test]
    fn ddr_atomic_words_and_bulk_snapshots() {
        let memory = Ddr::new(PAGE_BYTES);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for i in 0..200 {
                    let bytes = vec![if i % 2 == 0 { 0x55 } else { 0xaa }; PAGE_BYTES];
                    memory.write_buffer(DRAM_BASE, &bytes).unwrap();
                }
            });
            scope.spawn(|| {
                for _ in 0..100_000 {
                    assert!(matches!(
                        memory.read(DRAM_BASE + 128, Width::Double).unwrap(),
                        0 | 0x5555555555555555 | 0xaaaaaaaaaaaaaaaa
                    ));
                    assert!(matches!(
                        memory.read(DRAM_BASE + 131, Width::Double).unwrap(),
                        0 | 0x5555555555555555 | 0xaaaaaaaaaaaaaaaa
                    ));
                }
            });
            let mut bytes = vec![0; PAGE_BYTES];
            for _ in 0..200 {
                memory.read_buffer(DRAM_BASE, &mut bytes).unwrap();
                assert!(bytes.iter().all(|&b| b == bytes[0]));
            }
        });
        std::thread::scope(|scope| {
            for byte in 0..8 {
                let memory = &memory;
                scope.spawn(move || {
                    for _ in 0..1_000 {
                        memory.write(DRAM_BASE + byte, Width::Byte, byte + 1).unwrap();
                    }
                });
            }
        });
        assert_eq!(memory.read(DRAM_BASE, Width::Double).unwrap(), 0x0807060504030201);
    }
}

pub struct Pages {
    free: Vec<(u64, u64)>,
}

impl Pages {
    pub fn new(bytes: usize) -> Self {
        Self {
            free: vec![(DRAM_BASE, bytes as u64 & !4095)],
        }
    }

    pub fn allocate(&mut self, bytes: u64) -> Result<u64, String> {
        assert!(bytes > 0 && bytes % 4096 == 0);
        let index = self
            .free
            .iter()
            .position(|(_, size)| *size >= bytes)
            .ok_or("design DDR page pool exhausted")?;
        let (address, size) = self.free[index];
        if size == bytes {
            self.free.remove(index);
        } else {
            self.free[index] = (address + bytes, size - bytes);
        }
        Ok(address)
    }

    pub fn release(&mut self, address: u64, bytes: u64) {
        self.free.push((address, bytes));
        self.free.sort_unstable_by_key(|range| range.0);
        let mut index = 1;
        while index < self.free.len() {
            let end = self.free[index - 1].0 + self.free[index - 1].1;
            assert!(end <= self.free[index].0, "overlapping DDR page release");
            if end == self.free[index].0 {
                let bytes = self.free.remove(index).1;
                self.free[index - 1].1 += bytes;
            } else {
                index += 1;
            }
        }
    }
}
