use crate::{Engine, Fault, Memory};

impl Engine {
    pub(crate) fn vector_memory(&mut self, memory: &mut impl Memory) -> Result<(), Fault> {
        let insn = self.instruction;
        let illegal = self.illegal();
        let register = ((insn >> 7) & 31) as usize;
        let base = self.x[((insn >> 15) & 31) as usize];
        let rs2 = ((insn >> 20) & 31) as usize;
        let masked = insn & (1 << 25) == 0;
        let mode = (insn >> 26) & 3;
        let store = insn & 0x7f == 0x27;
        let bits = match (insn >> 12) & 7 {
            0 => 8,
            5 => 16,
            6 => 32,
            7 => 64,
            _ => return Err(illegal),
        };
        if bits > self.vector.elen {
            return Err(illegal);
        }
        let fields = (insn >> 29) as usize + 1;
        if mode == 0 && rs2 == 8 {
            if masked
                || insn & (1 << 28) != 0
                || !matches!(fields, 1 | 2 | 4 | 8)
                || !register.is_multiple_of(fields)
                || register + fields > 32
                || (store && bits != 8)
            {
                return Err(illegal);
            }
            let count = fields * self.vector.vlen / bits;
            for index in self.vector.vstart..count {
                let address = base.wrapping_add((index * bits / 8) as u64);
                let result = if store {
                    self.write(memory, address, bits / 8, self.vector.read(register, index, bits))
                        .map(|_| 0)
                } else {
                    self.read(memory, address, bits / 8)
                };
                match result {
                    Ok(value) => {
                        if !store {
                            self.vector.write(register, index, bits, value);
                        }
                    }
                    Err(fault) => {
                        self.vector.vstart = index;
                        return Err(fault);
                    }
                }
            }
            self.vector.vstart = 0;
            return Ok(());
        }
        if self.vector.vtype >> 31 != 0
            || fields != 1
            || insn & (1 << 28) != 0
            || (mode == 0 && rs2 != 0)
            || (!matches!(mode, 1 | 3) && !self.vector.group(register, bits))
            || (matches!(mode, 1 | 3)
                && (!self.vector.group(rs2, bits) || !self.vector.group(register, self.vector.sew())))
            || (!store && masked && register == 0)
        {
            return Err(illegal);
        }
        let element_bits = if matches!(mode, 1 | 3) { self.vector.sew() } else { bits };
        for index in self.vector.vstart..self.vector.vl {
            if masked && !self.vector.mask(0, index) {
                continue;
            }
            let address = match mode {
                0 => base.wrapping_add((index * bits / 8) as u64),
                1 | 3 => base.wrapping_add(self.vector.read(rs2, index, bits) as u64),
                2 => base.wrapping_add((index as u64).wrapping_mul(self.x[rs2])),
                _ => unreachable!(),
            };
            let result = if store {
                self.write(
                    memory,
                    address,
                    element_bits / 8,
                    self.vector.read(register, index, element_bits),
                )
                .map(|_| 0)
            } else {
                self.read(memory, address, element_bits / 8)
            };
            match result {
                Ok(value) => {
                    if !store {
                        self.vector.write(register, index, element_bits, value);
                    }
                }
                Err(fault) => {
                    self.vector.vstart = index;
                    return Err(fault);
                }
            }
        }
        self.vector.vstart = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Engine, Memory, MemoryError};
    struct ReadAddress(u64);
    impl Memory for ReadAddress {
        fn ball_command(&mut self, _: u32, _: u64, _: u64) -> Result<u64, MemoryError> { Err(MemoryError) }
        fn read(&mut self, address: u64, _: usize) -> Result<u64, MemoryError> {
            self.0 = address;
            Err(MemoryError)
        }
        fn write(&mut self, _: u64, _: usize, _: u64) -> Result<(), MemoryError> { Err(MemoryError) }
    }
    #[test]
    fn rv64_vector_memory_keeps_high_address_and_fault_value() {
        let mut engine = Engine::new(256, 64, 4096);
        engine.vector.vtype = 2 << 3;
        engine.vector.vl = 1;
        engine.x[10] = 0x1234567880000000;
        engine.instruction = (1 << 25) | (10 << 15) | (6 << 12) | (8 << 7) | 0x07;
        let mut memory = ReadAddress(0);
        let fault = engine.vector_memory(&mut memory).unwrap_err();
        assert_eq!(memory.0, 0x1234567880000000);
        assert_eq!((fault.cause, fault.value), (5, 0x1234567880000000));
    }
}
