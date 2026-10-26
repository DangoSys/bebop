use super::{Memory, MemoryError};

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
