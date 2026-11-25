use crate::riscv::bus::BusError;

pub struct LocalMemory {
    pub base: u64,
    pub bytes: Vec<u8>,
}
impl LocalMemory {
    pub fn new(base: u64, bytes: usize) -> Self {
        Self {
            base,
            bytes: vec![0xa5; bytes],
        }
    }
    pub fn offset(&self, address: u64, count: usize) -> Option<usize> {
        let offset = address.checked_sub(self.base)?;
        (offset.checked_add(count as u64)? <= self.bytes.len() as u64).then_some(offset as usize)
    }
    pub fn read(&self, address: u64, count: usize) -> Result<u64, BusError> {
        let offset = self.offset(address, count).ok_or(BusError)?;
        let mut data = [0; 8];
        data[..count].copy_from_slice(&self.bytes[offset..offset + count]);
        Ok(u64::from_le_bytes(data))
    }
    pub fn write(&mut self, address: u64, count: usize, value: u64) -> Result<(), BusError> {
        let offset = self.offset(address, count).ok_or(BusError)?;
        self.bytes[offset..offset + count].copy_from_slice(&value.to_le_bytes()[..count]);
        Ok(())
    }
}

/// The code and TLS stores of a hardware thread; its hart loads and saves them through TSS
/// transfers while the thread is idle.
pub struct Stores {
    pub code: LocalMemory,
    pub tls: LocalMemory,
}

impl Stores {
    pub fn new(params: &crate::configs::AntConfig) -> Self {
        let tls = params.tls.as_ref().expect("Ant TLS parameters");
        Self {
            code: LocalMemory::new(0, params.code_bytes as usize),
            tls: LocalMemory::new(tls.base, tls.bytes as usize),
        }
    }
}
