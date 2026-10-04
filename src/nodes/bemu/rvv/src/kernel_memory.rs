use crate::{Memory, MemoryError};

const CONSTANT_BASE: u32 = 0x80000000;
const STACK_BASE: u32 = 0x80001000;
const STACK_END: u32 = 0x80002000;

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
    fn read(&mut self, address: u32, bytes: usize) -> Result<u64, MemoryError> {
        if address < CONSTANT_BASE {
            return self.banks.read(address, bytes);
        }
        let (data, offset) = if address < STACK_BASE {
            (self.constants, (address - CONSTANT_BASE) as usize)
        } else if address < STACK_END {
            (self.stack.as_slice(), (address - STACK_BASE) as usize)
        } else {
            return Err(MemoryError);
        };
        let source = data.get(offset..offset + bytes).ok_or(MemoryError)?;
        let mut value = [0u8; 8];
        value[..bytes].copy_from_slice(source);
        Ok(u64::from_le_bytes(value))
    }

    fn write(&mut self, address: u32, bytes: usize, value: u64) -> Result<(), MemoryError> {
        if address < CONSTANT_BASE {
            return self.banks.write(address, bytes, value);
        }
        if !(STACK_BASE..STACK_END).contains(&address) {
            return Err(MemoryError);
        }
        let offset = (address - STACK_BASE) as usize;
        let target = self.stack.get_mut(offset..offset + bytes).ok_or(MemoryError)?;
        target.copy_from_slice(&value.to_le_bytes()[..bytes]);
        Ok(())
    }
}
