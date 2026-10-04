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
                let address = base.wrapping_add((index * bits / 8) as u32);
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
                0 => base.wrapping_add((index * bits / 8) as u32),
                1 | 3 => base.wrapping_add(self.vector.read(rs2, index, bits) as u32),
                2 => base.wrapping_add((index as u32).wrapping_mul(self.x[rs2])),
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
