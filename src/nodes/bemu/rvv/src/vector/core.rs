use crate::{Engine, Fault};

pub(crate) struct Vector {
    pub(crate) vlen: usize,
    pub(crate) elen: usize,
    pub(crate) vl: usize,
    pub(crate) vtype: u32,
    pub(crate) vstart: usize,
    pub(crate) registers: Vec<u8>,
}

impl Vector {
    pub(crate) fn new(vlen: usize, elen: usize) -> Self {
        Self {
            vlen,
            elen,
            vl: 0,
            vtype: 1 << 31,
            vstart: 0,
            registers: vec![0; 32 * vlen / 8],
        }
    }
    pub(crate) fn sew(&self) -> usize {
        8 << ((self.vtype >> 3) & 7)
    }
    pub(crate) fn vlmax(&self) -> usize {
        let base = self.vlen / self.sew();
        match self.vtype & 7 {
            0..=3 => base << (self.vtype & 7),
            5..=7 => base >> (8 - (self.vtype & 7)),
            _ => 0,
        }
    }
    pub(crate) fn group(&self, register: usize, bits: usize) -> bool {
        if bits > self.elen {
            return false;
        }
        let exponent = match self.vtype & 7 {
            0..=3 => (self.vtype & 7) as i32,
            5..=7 => (self.vtype & 7) as i32 - 8,
            _ => return false,
        };
        let ratio = exponent + bits.ilog2() as i32 - self.sew().ilog2() as i32;
        if !(-3..=3).contains(&ratio) {
            return false;
        }
        let count = 1usize << ratio.max(0);
        register.is_multiple_of(count) && register + count <= 32
    }
    pub(crate) fn read(&self, register: usize, element: usize, bits: usize) -> u64 {
        let offset = register * self.vlen / 8 + element * bits / 8;
        match bits {
            8 => self.registers[offset] as u64,
            16 => u16::from_le_bytes(self.registers[offset..offset + 2].try_into().unwrap()) as u64,
            32 => u32::from_le_bytes(self.registers[offset..offset + 4].try_into().unwrap()) as u64,
            64 => u64::from_le_bytes(self.registers[offset..offset + 8].try_into().unwrap()),
            _ => unreachable!(),
        }
    }
    pub(crate) fn write(&mut self, register: usize, element: usize, bits: usize, value: u64) {
        let offset = register * self.vlen / 8 + element * bits / 8;
        match bits {
            8 => self.registers[offset] = value as u8,
            16 => self.registers[offset..offset + 2].copy_from_slice(&(value as u16).to_le_bytes()),
            32 => self.registers[offset..offset + 4].copy_from_slice(&(value as u32).to_le_bytes()),
            64 => self.registers[offset..offset + 8].copy_from_slice(&value.to_le_bytes()),
            _ => unreachable!(),
        }
    }
    pub(crate) fn mask(&self, register: usize, element: usize) -> bool {
        self.registers[register * self.vlen / 8 + element / 8] & (1 << (element % 8)) != 0
    }
    pub(crate) fn write_mask(&mut self, register: usize, element: usize, value: bool) {
        let byte = &mut self.registers[register * self.vlen / 8 + element / 8];
        let bit = 1 << (element % 8);
        *byte = (*byte & !bit) | if value { bit } else { 0 };
    }
}

impl Engine {
    pub(crate) fn vector_execute(&mut self) -> Result<(), Fault> {
        let instruction = self.instruction;
        let kind = (instruction >> 12) & 7;
        if kind == 7 {
            return self.configure_vector();
        }
        if self.vector.vtype >> 31 != 0 {
            return Err(self.illegal());
        }
        match kind {
            1 | 5 => self.vector_float()?,
            0 | 2 | 3 | 4 | 6 => self.vector_integer()?,
            _ => return Err(self.illegal()),
        }
        self.vector.vstart = 0;
        Ok(())
    }

    pub(crate) fn configure_vector(&mut self) -> Result<(), Fault> {
        let instruction = self.instruction;
        let rs1 = ((instruction >> 15) & 31) as usize;
        let rd = ((instruction >> 7) & 31) as usize;
        let immediate = instruction >> 30 == 3;
        let vtype = if instruction >> 31 == 0 {
            instruction >> 20
        } else if immediate {
            (instruction >> 20) & 0x3ff
        } else if instruction >> 25 == 0x40 {
            self.x[((instruction >> 20) & 31) as usize]
        } else {
            return Err(self.illegal());
        };
        let sew = 8u32 << ((vtype >> 3) & 7);
        let lmul = vtype & 7;
        if vtype >> 8 != 0
            || sew as usize > self.vector.elen
            || lmul == 4
            || (lmul >= 5 && sew as usize > (self.vector.elen >> (8 - lmul))) {
            self.vector.vtype = 1 << 31;
            self.vector.vl = 0;
        } else {
            let old_max = self.vector.vlmax();
            let old_type = self.vector.vtype;
            self.vector.vtype = vtype;
            let max = self.vector.vlmax();
            if !immediate && rs1 == 0 && rd == 0 && max != old_max {
                self.vector.vtype = old_type;
                return Err(self.illegal());
            }
            let avl = if immediate {
                rs1
            } else if rs1 != 0 {
                self.x[rs1] as usize
            } else if rd != 0 {
                max
            } else {
                self.vector.vl
            };
            self.vector.vl = avl.min(max);
        }
        self.x[rd] = self.vector.vl as u32;
        self.vector.vstart = 0;
        Ok(())
    }
}
