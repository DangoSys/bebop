use crate::root::platform::DRAM_BASE;

pub(crate) struct Pages {
    free: Vec<(u64, u64)>,
}

impl Pages {
    pub(crate) fn new(bytes: usize) -> Self {
        Self {
            free: vec![(DRAM_BASE, bytes as u64 & !4095)],
        }
    }

    pub(crate) fn allocate(&mut self, bytes: u64) -> Result<u64, String> {
        assert!(bytes > 0 && bytes % 4096 == 0);
        let index = self
            .free
            .iter()
            .position(|(_, size)| *size >= bytes)
            .ok_or("chip DDR page pool exhausted")?;
        let (address, size) = self.free[index];
        if size == bytes {
            self.free.remove(index);
        } else {
            self.free[index] = (address + bytes, size - bytes);
        }
        Ok(address)
    }

    pub(crate) fn release(&mut self, address: u64, bytes: u64) {
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
