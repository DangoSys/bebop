use crate::{Engine, Fault, Memory};

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
                (1, 4) => if b == 0 { u64::MAX } else { (a as i64).wrapping_div(b as i64) as u64 },
                (1, 5) => a.checked_div(b).unwrap_or(u64::MAX),
                (1, 6) => if b == 0 { a } else { (a as i64).wrapping_rem(b as i64) as u64 },
                (1, 7) => if b == 0 { a } else { a % b },
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
                    (1, 4) => if b == 0 { u32::MAX } else { (a as i32).wrapping_div(b as i32) as u32 },
                    (1, 5) => a.checked_div(b).unwrap_or(u32::MAX),
                    (1, 6) => if b == 0 { a } else { (a as i32).wrapping_rem(b as i32) as u32 },
                    (1, 7) => if b == 0 { a } else { a % b },
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
                let offset = ((insn >> 7) & 0x1e) | ((insn >> 20) & 0x7e0)
                    | ((insn << 4) & 0x800) | ((insn >> 19) & 0x1000);
                if taken {
                    next = self.code_address((self.pc as u64).wrapping_add(((offset as i32) << 19 >> 19) as i64 as u64))?;
                }
                self.pc = next;
                return Ok(());
            }
            0x6f => {
                let offset = ((insn >> 20) & 0x7fe) | ((insn >> 9) & 0x800)
                    | (insn & 0xff000) | ((insn >> 11) & 0x100000);
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
                    0xc21 => if self.vector.vtype >> 31 != 0 { 1 << 63 } else { self.vector.vtype as u64 },
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

#[cfg(test)]
mod tests {
    use crate::{Engine, Memory, MemoryError};
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Bytes(BTreeMap<u64, u8>);
    impl Memory for Bytes {
        fn ball_command(&mut self, _: u32, _: u64, _: u64) -> Result<u64, MemoryError> { Err(MemoryError) }
        fn read(&mut self, address: u64, bytes: usize) -> Result<u64, MemoryError> {
            let mut result = 0;
            for index in 0..bytes {
                result |= (*self.0.get(&(address + index as u64)).ok_or(MemoryError)? as u64) << (8 * index);
            }
            Ok(result)
        }
        fn write(&mut self, address: u64, bytes: usize, value: u64) -> Result<(), MemoryError> {
            for index in 0..bytes {
                self.0.insert(address + index as u64, (value >> (8 * index)) as u8);
            }
            Ok(())
        }
    }
    fn immediate(opcode: u32, funct: u32, imm: u32) -> u32 {
        (imm << 20) | (10 << 15) | (funct << 12) | (12 << 7) | opcode
    }
    fn binary(opcode: u32, op: u32, funct: u32) -> u32 {
        (op << 25) | (11 << 20) | (10 << 15) | (funct << 12) | (12 << 7) | opcode
    }
    fn execute(insn: u32, a: u64, b: u64) -> Result<u64, crate::Fault> {
        let mut engine = Engine::new(256, 64, 4096);
        engine.x[10] = a;
        engine.x[11] = b;
        engine.instruction = insn;
        engine.scalar_execute(&mut Bytes::default())?;
        Ok(engine.x[12])
    }
    #[test]
    fn rv64_shifts_and_sign_extensions() {
        assert_eq!(execute(immediate(0x13, 1, 63), 1, 0).unwrap(), 1 << 63);
        assert_eq!(execute(immediate(0x13, 5, 63), 1 << 63, 0).unwrap(), 1);
        assert_eq!(execute(immediate(0x13, 5, 0x400 | 63), 1 << 63, 0).unwrap(), u64::MAX);
        assert_eq!(execute(binary(0x33, 0, 1), 1, 63).unwrap(), 1 << 63);
        assert_eq!(execute(immediate(0x1b, 0, 1), 0x7fffffff, 0).unwrap(), 0xffffffff80000000);
        assert_eq!(execute(immediate(0x1b, 5, 1), 0xffffffff80000000, 0).unwrap(), 0x40000000);
        assert_eq!(execute(immediate(0x1b, 5, 0x401), 0x80000000, 0).unwrap(), 0xffffffffc0000000);
        assert_eq!(execute(immediate(0x1b, 1, 32), 1, 0).unwrap_err().cause, 2);
        assert_eq!(execute(0x80000637, 0, 0).unwrap(), 0xffffffff80000000);
        assert_eq!(execute(0x80000617, 0, 0).unwrap(), 0xffffffff80000000);
    }
    #[test]
    fn rv64_multiply_divide_words() {
        assert_eq!(execute(binary(0x33, 1, 1), u64::MAX, 2).unwrap(), u64::MAX);
        assert_eq!(execute(binary(0x33, 1, 2), u64::MAX, u64::MAX).unwrap(), u64::MAX);
        assert_eq!(execute(binary(0x33, 1, 3), u64::MAX, u64::MAX).unwrap(), u64::MAX - 1);
        assert_eq!(execute(binary(0x33, 1, 4), 1 << 63, u64::MAX).unwrap(), 1 << 63);
        assert_eq!(execute(binary(0x33, 1, 6), 1 << 63, u64::MAX).unwrap(), 0);
        assert_eq!(execute(binary(0x33, 1, 5), 0x123456789abcdef0, 0).unwrap(), u64::MAX);
        assert_eq!(execute(binary(0x3b, 1, 5), 0xffffffff, 1).unwrap(), u64::MAX);
        assert_eq!(execute(binary(0x3b, 1, 4), 0x80000000, 0xffffffff).unwrap(), 0xffffffff80000000);
        assert_eq!(execute(binary(0x3b, 1, 0), 0x1234567880000000, 2).unwrap(), 0);
    }
    #[test]
    fn rv64_memory_and_fault_addresses() {
        let mut memory = Bytes::default();
        let address = 0x1234567880000000;
        memory.write(address, 8, 0xfedcba9887654321).unwrap();
        let mut engine = Engine::new(256, 64, 4096);
        engine.x[10] = address;
        for (funct, expected) in [(3, 0xfedcba9887654321), (2, 0xffffffff87654321), (6, 0x87654321)] {
            engine.instruction = immediate(0x03, funct, 0);
            engine.scalar_execute(&mut memory).unwrap();
            assert_eq!(engine.x[12], expected);
        }
        engine.x[11] = 0xdeadbeef98765432;
        engine.instruction = (11 << 20) | (10 << 15) | (3 << 12) | (8 << 7) | 0x23;
        engine.scalar_execute(&mut memory).unwrap();
        assert_eq!(memory.read(address + 8, 8).unwrap(), 0xdeadbeef98765432);
        engine.x[10] = address + 4096;
        engine.instruction = immediate(0x03, 3, 0);
        let fault = engine.scalar_execute(&mut memory).unwrap_err();
        assert_eq!((fault.cause, fault.value), (5, address + 4096));
    }
    #[test]
    fn rv64_jump_rejects_upper_bits_without_truncation() {
        let mut engine = Engine::new(256, 64, 4096);
        engine.x[10] = 0x1234567800000000;
        engine.instruction = immediate(0x67, 0, 0);
        let fault = engine.scalar_execute(&mut Bytes::default()).unwrap_err();
        assert_eq!((fault.cause, fault.value), (1, 0x1234567800000000));
        engine.x[10] = 2;
        assert_eq!(engine.scalar_execute(&mut Bytes::default()).unwrap_err().cause, 0);
    }
    #[test]
    fn rv64_run_preserves_argument_and_stack_width() {
        // sd a1,0(a0); sd sp,8(a0); jalr x0,0(ra)
        let words = [0x00b53023u32, 0x00253423, 0x00008067];
        let program: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let mut engine = Engine::new(256, 64, 4096);
        engine.load_program(0, &program);
        let mut memory = Bytes::default();
        engine.run(0, 0, 12, [0x100000000, 0xfedcba9876543210, 0, 0, 0, 0, 0, 0], 0x40002000, &mut memory).unwrap();
        assert_eq!(memory.read(0x100000000, 8).unwrap(), 0xfedcba9876543210);
        assert_eq!(memory.read(0x100000008, 8).unwrap(), 0x40002000);
    }
    #[test]
    fn rv64_kernel_ball_commands_keep_operands_returns_and_order() {
        struct Commands { memory: Bytes, calls: Vec<(u32, u64, u64)> }
        impl Memory for Commands {
            fn read(&mut self, address: u64, bytes: usize) -> Result<u64, MemoryError> { self.memory.read(address, bytes) }
            fn write(&mut self, address: u64, bytes: usize, value: u64) -> Result<(), MemoryError> { self.memory.write(address, bytes, value) }
            fn ball_command(&mut self, funct7: u32, rs1: u64, rs2: u64) -> Result<u64, MemoryError> {
                self.calls.push((funct7, rs1, rs2));
                Ok(match funct7 { 0x41 => 0x876543219abcdef0, 0x42 => 0xfedcba9876543210, _ => return Err(MemoryError) })
            }
        }
        let words = [
            (0x41u32 << 25) | (11 << 20) | (10 << 15) | (3 << 12) | (12 << 7) | 0x7b,
            (0x42 << 25) | (11 << 20) | (12 << 15) | (3 << 12) | (10 << 7) | 0x7b,
            (10 << 20) | (13 << 15) | (3 << 12) | 0x23,
            0x00008067,
        ];
        let program: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let mut engine = Engine::new(256, 64, 4096);
        engine.load_program(0, &program);
        let mut memory = Commands { memory: Bytes::default(), calls: Vec::new() };
        engine.run(0, 0, 16, [0x1234567800000001, 0xabcdef0100000002, 0, 0x100000000, 0, 0, 0, 0], 0x40002000, &mut memory).unwrap();
        assert_eq!(memory.calls, [(0x41, 0x1234567800000001, 0xabcdef0100000002), (0x42, 0x876543219abcdef0, 0xabcdef0100000002)]);
        assert_eq!(memory.memory.read(0x100000000, 8).unwrap(), 0xfedcba9876543210);
        assert_eq!(execute(words[0], 0x1234567800000001, 0xabcdef0100000002).unwrap_err().cause, 2);
    }

}
