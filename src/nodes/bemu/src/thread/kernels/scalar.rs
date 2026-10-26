use super::{Engine, Fault, Memory};

impl Engine {
    pub(super) fn scalar_execute(&mut self, memory: &mut impl Memory) -> Result<(), Fault> {
        let insn = self.instruction;
        let opcode = insn & 0x7f;
        let rd = ((insn >> 7) & 31) as usize;
        let funct = (insn >> 12) & 7;
        let rs1 = ((insn >> 15) & 31) as usize;
        let rs2 = ((insn >> 20) & 31) as usize;
        let op = insn >> 25;
        let a = self.x[rs1];
        let b = self.x[rs2];
        let imm = ((insn as i32) >> 20) as i64 as u64;
        let shift = (insn >> 20) & 63;
        let mut next = self.pc + 4;
        let value = match opcode {
            0x37 => (insn & 0xfffff000) as i32 as i64 as u64,
            0x17 => (self.pc as u64).wrapping_add((insn & 0xfffff000) as i32 as i64 as u64),
            0x13 => match funct {
                0 => a.wrapping_add(imm),
                2 => u64::from((a as i64) < imm as i64),
                3 => u64::from(a < imm),
                4 => a ^ imm,
                6 => a | imm,
                7 => a & imm,
                1 if op >> 1 == 0 => a << shift,
                5 if op >> 1 == 0 => a >> shift,
                5 if op >> 1 == 0x10 => ((a as i64) >> shift) as u64,
                _ => return Err(self.illegal()),
            },
            0x1b => {
                let result = match funct {
                    0 => (a as u32).wrapping_add(imm as u32),
                    1 if op == 0 => (a as u32) << rs2,
                    5 if op == 0 => (a as u32) >> rs2,
                    5 if op == 0x20 => ((a as i32) >> rs2) as u32,
                    _ => return Err(self.illegal()),
                };
                result as i32 as i64 as u64
            }
            0x33 => match (op, funct) {
                (0, 0) => a.wrapping_add(b),
                (0x20, 0) => a.wrapping_sub(b),
                (0, 1) => a << (b & 63),
                (0, 2) => u64::from((a as i64) < b as i64),
                (0, 3) => u64::from(a < b),
                (0, 4) => a ^ b,
                (0, 5) => a >> (b & 63),
                (0x20, 5) => ((a as i64) >> (b & 63)) as u64,
                (0, 6) => a | b,
                (0, 7) => a & b,
                (1, 0) => a.wrapping_mul(b),
                (1, 1) => (((a as i64 as i128) * (b as i64 as i128)) >> 64) as u64,
                (1, 2) => (((a as i64 as i128) * (b as i128)) >> 64) as u64,
                (1, 3) => (((a as u128) * (b as u128)) >> 64) as u64,
                (1, 4) => {
                    if b == 0 {
                        u64::MAX
                    } else {
                        (a as i64).wrapping_div(b as i64) as u64
                    }
                }
                (1, 5) => a.checked_div(b).unwrap_or(u64::MAX),
                (1, 6) => {
                    if b == 0 {
                        a
                    } else {
                        (a as i64).wrapping_rem(b as i64) as u64
                    }
                }
                (1, 7) => {
                    if b == 0 {
                        a
                    } else {
                        a % b
                    }
                }
                _ => return Err(self.illegal()),
            },
            0x3b => {
                let a = a as u32;
                let b = b as u32;
                let result = match (op, funct) {
                    (0, 0) => a.wrapping_add(b),
                    (0x20, 0) => a.wrapping_sub(b),
                    (0, 1) => a << (b & 31),
                    (0, 5) => a >> (b & 31),
                    (0x20, 5) => ((a as i32) >> (b & 31)) as u32,
                    (1, 0) => a.wrapping_mul(b),
                    (1, 4) => {
                        if b == 0 {
                            u32::MAX
                        } else {
                            (a as i32).wrapping_div(b as i32) as u32
                        }
                    }
                    (1, 5) => a.checked_div(b).unwrap_or(u32::MAX),
                    (1, 6) => {
                        if b == 0 {
                            a
                        } else {
                            (a as i32).wrapping_rem(b as i32) as u32
                        }
                    }
                    (1, 7) => {
                        if b == 0 {
                            a
                        } else {
                            a % b
                        }
                    }
                    _ => return Err(self.illegal()),
                };
                result as i32 as i64 as u64
            }
            0x03 => {
                let address = a.wrapping_add(imm);
                match funct {
                    0 => self.read(memory, address, 1)? as i8 as i64 as u64,
                    1 => self.read(memory, address, 2)? as i16 as i64 as u64,
                    2 => self.read(memory, address, 4)? as i32 as i64 as u64,
                    3 => self.read(memory, address, 8)?,
                    4 => self.read(memory, address, 1)?,
                    5 => self.read(memory, address, 2)?,
                    6 => self.read(memory, address, 4)?,
                    _ => return Err(self.illegal()),
                }
            }
            0x23 | 0x27 => {
                let offset = ((((insn >> 7) & 31) | ((insn >> 20) & 0xfe0)) as i32) << 20 >> 20;
                let bytes = match (opcode, funct) {
                    (0x23, 0) => 1,
                    (0x23, 1) => 2,
                    (0x23, 2) | (0x27, 2) => 4,
                    (0x23, 3) | (0x27, 3) => 8,
                    _ => return Err(self.illegal()),
                };
                if opcode == 0x27 && bytes * 8 > self.vector.elen {
                    return Err(self.illegal());
                }
                let value = if opcode == 0x27 { self.f[rs2] } else { b };
                self.write(memory, a.wrapping_add(offset as i64 as u64), bytes, value)?;
                self.pc = next;
                return Ok(());
            }
            0x07 if matches!(funct, 2 | 3) => {
                if funct == 3 && self.vector.elen < 64 {
                    return Err(self.illegal());
                }
                let bits = self.read(memory, a.wrapping_add(imm), if funct == 2 { 4 } else { 8 })?;
                self.f[rd] = if funct == 2 { bits | 0xffffffff00000000 } else { bits };
                self.pc = next;
                return Ok(());
            }
            0x43 | 0x47 | 0x4b | 0x4f | 0x53 => {
                self.scalar_float()?;
                self.pc = next;
                return Ok(());
            }
            0x63 => {
                let taken = match funct {
                    0 => a == b,
                    1 => a != b,
                    4 => (a as i64) < b as i64,
                    5 => (a as i64) >= b as i64,
                    6 => a < b,
                    7 => a >= b,
                    _ => return Err(self.illegal()),
                };
                let offset =
                    ((insn >> 7) & 0x1e) | ((insn >> 20) & 0x7e0) | ((insn << 4) & 0x800) | ((insn >> 19) & 0x1000);
                if taken {
                    next =
                        self.code_address((self.pc as u64).wrapping_add(((offset as i32) << 19 >> 19) as i64 as u64))?;
                }
                self.pc = next;
                return Ok(());
            }
            0x6f => {
                let offset =
                    ((insn >> 20) & 0x7fe) | ((insn >> 9) & 0x800) | (insn & 0xff000) | ((insn >> 11) & 0x100000);
                next = self.code_address((self.pc as u64).wrapping_add(((offset as i32) << 11 >> 11) as i64 as u64))?;
                (self.pc as u64) + 4
            }
            0x67 if funct == 0 => {
                next = self.code_address(a.wrapping_add(imm) & !1)?;
                (self.pc as u64) + 4
            }
            0x7b if funct == 3 => memory.ball_command(op, a, b).map_err(|_| self.illegal())?,
            0x0f if funct <= 1 => {
                self.pc = next;
                return Ok(());
            }
            0x73 if funct != 0 && funct != 4 => {
                let address = insn >> 20;
                let old = match address {
                    1 => (self.fcsr & 31) as u64,
                    2 => (self.fcsr >> 5) as u64,
                    3 => self.fcsr as u64,
                    0x008 => self.vector.vstart as u64,
                    0xc20 => self.vector.vl as u64,
                    0xc21 => {
                        if self.vector.vtype >> 31 != 0 {
                            1 << 63
                        } else {
                            self.vector.vtype as u64
                        }
                    }
                    0xc22 => (self.vector.vlen / 8) as u64,
                    _ => return Err(self.illegal()),
                };
                let operand = if funct & 4 != 0 { rs1 as u64 } else { a };
                if funct & 3 == 1 || rs1 != 0 {
                    let value = match funct & 3 {
                        1 => operand,
                        2 => old | operand,
                        3 => old & !operand,
                        _ => unreachable!(),
                    };
                    match address {
                        1 => self.fcsr = (self.fcsr & !31) | (value as u8 & 31),
                        2 => self.fcsr = (self.fcsr & 31) | ((value as u8 & 7) << 5),
                        3 => self.fcsr = value as u8,
                        0x008 => self.vector.vstart = value as usize & (self.vector.vlen - 1),
                        _ => return Err(self.illegal()),
                    }
                }
                old
            }
            _ => return Err(self.illegal()),
        };
        self.x[rd] = value;
        self.pc = next;
        Ok(())
    }
}
