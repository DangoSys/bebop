use crate::{Memory, MemoryError};

const CONSTANT_BASE: u64 = 0x40000000;
const STACK_BASE: u64 = 0x40001000;
const STACK_END: u64 = 0x40002000;

pub(crate) struct KernelMemory<'a, M> {
    banks: &'a mut M,
    constants: &'a [u8],
    stack: Vec<u8>,
}

impl<'a, M: Memory> KernelMemory<'a, M> {
    pub(crate) fn new(banks: &'a mut M, constants: &'a [u8]) -> Self {
        Self {
            banks,
            constants,
            stack: vec![0; 4096],
        }
    }
}

impl<M: Memory> Memory for KernelMemory<'_, M> {
    fn ball_command(&mut self, funct7: u32, rs1: u64, rs2: u64) -> Result<u64, MemoryError> {
        self.banks.ball_command(funct7, rs1, rs2)
    }
    fn read(&mut self, address: u64, bytes: usize) -> Result<u64, MemoryError> {
        let (data, offset) = if (CONSTANT_BASE..STACK_BASE).contains(&address) {
            (self.constants, (address - CONSTANT_BASE) as usize)
        } else if (STACK_BASE..STACK_END).contains(&address) {
            (self.stack.as_slice(), (address - STACK_BASE) as usize)
        } else {
            return self.banks.read(address, bytes);
        };
        let source = data.get(offset..offset + bytes).ok_or(MemoryError)?;
        let mut value = [0u8; 8];
        value[..bytes].copy_from_slice(source);
        Ok(u64::from_le_bytes(value))
    }

    fn write(&mut self, address: u64, bytes: usize, value: u64) -> Result<(), MemoryError> {
        if (CONSTANT_BASE..STACK_BASE).contains(&address) {
            return Err(MemoryError);
        }
        if !(STACK_BASE..STACK_END).contains(&address) {
            return self.banks.write(address, bytes, value);
        }
        let offset = (address - STACK_BASE) as usize;
        let target = self.stack.get_mut(offset..offset + bytes).ok_or(MemoryError)?;
        target.copy_from_slice(&value.to_le_bytes()[..bytes]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct External { address: u64, value: u64 }
    impl Memory for External {
        fn ball_command(&mut self, _: u32, _: u64, _: u64) -> Result<u64, MemoryError> { Err(MemoryError) }
        fn read(&mut self, address: u64, _: usize) -> Result<u64, MemoryError> {
            self.address = address;
            Ok(self.value)
        }
        fn write(&mut self, address: u64, _: usize, value: u64) -> Result<(), MemoryError> {
            self.address = address;
            self.value = value;
            Ok(())
        }
    }
    #[test]
    fn rv64_kernel_memory_regions_and_external_addresses() {
        let mut external = External { address: 0, value: 0xabcdef0123456789 };
        let constants = 0x123456789abcdef0u64.to_le_bytes();
        let mut memory = KernelMemory::new(&mut external, &constants);
        assert_eq!(memory.read(CONSTANT_BASE, 8).unwrap(), 0x123456789abcdef0);
        assert_eq!(memory.write(CONSTANT_BASE, 8, 0), Err(MemoryError));
        memory.write(STACK_BASE + 16, 8, 0xdeadbeef87654321).unwrap();
        assert_eq!(memory.read(STACK_BASE + 16, 8).unwrap(), 0xdeadbeef87654321);
        assert_eq!(memory.read(STACK_END - 4, 8), Err(MemoryError));
        let high = 0x1234567800000000;
        assert_eq!(memory.read(high, 8).unwrap(), 0xabcdef0123456789);
        memory.write(high + 8, 8, 0xfeedbeef98765432).unwrap();
        assert_eq!((external.address, external.value), (high + 8, 0xfeedbeef98765432));
    }
}
