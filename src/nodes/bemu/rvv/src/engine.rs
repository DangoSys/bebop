use crate::kernel_memory::KernelMemory;
use crate::vector::Vector;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fault {
    pub pc: u32,
    pub instruction: u32,
    pub cause: u32,
    pub value: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryError;

pub trait Memory {
    fn read(&mut self, address: u32, bytes: usize) -> Result<u64, MemoryError>;
    fn write(&mut self, address: u32, bytes: usize, value: u64) -> Result<(), MemoryError>;
}

pub struct Engine {
    pub(crate) x: [u32; 32],
    pub(crate) f: [u64; 32],
    pub(crate) fcsr: u8,
    pub(crate) vector: Vector,
    pub(crate) programs: [Vec<u8>; 2],
    pub(crate) loaded: [usize; 2],
    constants: [Vec<u8>; 2],
    pub(crate) pc: u32,
    pub(crate) instruction: u32,
}

impl Engine {
    pub fn new(vlen_bits: usize, elen_bits: usize, instruction_bytes: usize) -> Self {
        assert!(vlen_bits >= 128 && vlen_bits.is_power_of_two());
        assert!(matches!(elen_bits, 32 | 64));
        assert!(instruction_bytes > 0 && instruction_bytes.is_multiple_of(4));
        Self {
            x: [0; 32],
            f: [0; 32],
            fcsr: 0,
            vector: Vector::new(vlen_bits, elen_bits),
            programs: [vec![0; instruction_bytes], vec![0; instruction_bytes]],
            loaded: [0; 2],
            constants: [Vec::new(), Vec::new()],
            pc: 0,
            instruction: 0,
        }
    }

    pub fn load_program(&mut self, buffer: usize, bytes: &[u8]) {
        assert!(bytes.len().is_multiple_of(4));
        self.programs[buffer][..bytes.len()].copy_from_slice(bytes);
        self.loaded[buffer] = bytes.len();
    }

    pub fn load_constants(&mut self, buffer: usize, bytes: &[u8]) {
        assert!(bytes.len() <= 4096);
        self.constants[buffer] = bytes.to_vec();
    }

    pub(crate) fn fault(&self, cause: u32, value: u32) -> Fault {
        Fault {
            pc: self.pc,
            instruction: self.instruction,
            cause,
            value,
        }
    }

    pub(crate) fn illegal(&self) -> Fault {
        self.fault(2, self.instruction)
    }

    pub(crate) fn read(&self, memory: &mut impl Memory, address: u32, bytes: usize) -> Result<u64, Fault> {
        memory.read(address, bytes).map_err(|_| self.fault(5, address))
    }

    pub(crate) fn write(&self, memory: &mut impl Memory, address: u32, bytes: usize, value: u64) -> Result<(), Fault> {
        memory.write(address, bytes, value).map_err(|_| self.fault(7, address))
    }

    pub fn run(
        &mut self,
        instruction_buffer: usize,
        entry: u32,
        end: u32,
        args: [u32; 8],
        stack_top: u32,
        memory: &mut impl Memory,
    ) -> Result<(), Fault> {
        self.pc = entry;
        self.instruction = 0;
        if entry & 3 != 0 || end & 3 != 0 || entry >= end || end as usize > self.loaded[instruction_buffer] {
            return Err(self.fault(1, entry));
        }
        self.x.fill(0);
        self.f.fill(0);
        self.fcsr = 0;
        self.x[1] = end;
        self.x[2] = stack_top;
        self.x[10..18].copy_from_slice(&args);
        let constants = self.constants[instruction_buffer].clone();
        let mut memory = KernelMemory::new(memory, &constants);
        while self.pc != end {
            if self.pc & 3 != 0 {
                return Err(self.fault(0, self.pc));
            }
            let offset = self.pc as usize;
            let bytes = self.programs[instruction_buffer]
                .get(offset..offset + 4)
                .filter(|_| offset + 4 <= self.loaded[instruction_buffer])
                .ok_or_else(|| self.fault(1, self.pc))?;
            self.instruction = u32::from_le_bytes(bytes.try_into().unwrap());
            let opcode = self.instruction & 0x7f;
            let funct = (self.instruction >> 12) & 7;
            match opcode {
                0x57 => {
                    self.vector_execute()?;
                    self.pc = self.pc.wrapping_add(4);
                }
                0x07 | 0x27 if matches!(funct, 0 | 5 | 6 | 7) => {
                    self.vector_memory(&mut memory)?;
                    self.pc = self.pc.wrapping_add(4);
                }
                _ => self.scalar_execute(&mut memory)?,
            }
            self.x[0] = 0;
        }
        Ok(())
    }
}
