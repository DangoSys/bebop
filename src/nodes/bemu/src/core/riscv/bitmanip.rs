// Encodings: RISC-V International riscv-opcodes rv{32,64}_zba/zbb/zbs.
pub(crate) fn execute(insn: u32, lhs: u64, rhs: u64, bits: u32) -> Option<u64> {
    assert!(matches!(bits, 32 | 64));
    let mask = u64::MAX >> (64 - bits);
    let a = lhs & mask;
    let b = rhs & mask;
    let opcode = insn & 0x7f;
    let funct = (insn >> 12) & 7;
    let top = insn >> 25;
    let immediate = insn >> 20;
    let shift = (b & (bits as u64 - 1)) as u32;
    let signed = |value: u64| ((value << (64 - bits)) as i64) >> (64 - bits);
    let word = |value: u32| value as i32 as u64;
    let rotate = |value: u64, amount: u32, left: bool| {
        let opposite = (bits - amount) % bits;
        if left {
            (value << amount) | (value >> opposite)
        } else {
            (value >> amount) | (value << opposite)
        }
    };
    let result = match (opcode, top, funct) {
        (0x33, 0x10, 2 | 4 | 6) => (a << (funct / 2)).wrapping_add(b),
        (0x3b, 0x04, 0) if bits == 64 => (lhs as u32 as u64).wrapping_add(rhs),
        (0x3b, 0x10, 2 | 4 | 6) if bits == 64 => ((lhs as u32 as u64) << (funct / 2)).wrapping_add(rhs),
        (0x1b, _, 1) if bits == 64 && insn >> 26 == 2 => (lhs as u32 as u64) << (immediate & 63),
        (0x33, 0x20, 7) => a & !b,
        (0x33, 0x20, 6) => a | !b,
        (0x33, 0x20, 4) => !(a ^ b),
        (0x33, 0x05, 4) => {
            if signed(a) < signed(b) {
                a
            } else {
                b
            }
        }
        (0x33, 0x05, 5) => a.min(b),
        (0x33, 0x05, 6) => {
            if signed(a) > signed(b) {
                a
            } else {
                b
            }
        }
        (0x33, 0x05, 7) => a.max(b),
        (0x33, 0x30, 1 | 5) => rotate(a, shift, funct == 1),
        (0x3b, 0x30, 1) if bits == 64 => word((a as u32).rotate_left(shift & 31)),
        (0x3b, 0x30, 5) if bits == 64 => word((a as u32).rotate_right(shift & 31)),
        (0x33, 0x04, 4) if bits == 32 && (insn >> 20) & 31 == 0 => a & 0xffff,
        (0x3b, 0x04, 4) if bits == 64 && (insn >> 20) & 31 == 0 => a & 0xffff,
        (0x13, _, 1) if immediate == 0x600 => (a.leading_zeros() - (64 - bits)) as u64,
        (0x13, _, 1) if immediate == 0x601 => a.trailing_zeros().min(bits) as u64,
        (0x13, _, 1) if immediate == 0x602 => a.count_ones() as u64,
        (0x13, _, 1) if immediate == 0x604 => a as i8 as i64 as u64,
        (0x13, _, 1) if immediate == 0x605 => a as i16 as i64 as u64,
        (0x1b, _, 1) if bits == 64 && immediate == 0x600 => (a as u32).leading_zeros() as u64,
        (0x1b, _, 1) if bits == 64 && immediate == 0x601 => (a as u32).trailing_zeros() as u64,
        (0x1b, _, 1) if bits == 64 && immediate == 0x602 => (a as u32).count_ones() as u64,
        (0x13, _, 5) if insn >> 26 == 0x18 && (bits == 64 || immediate & 32 == 0) => {
            rotate(a, immediate & (bits - 1), false)
        }
        (0x1b, 0x30, 5) if bits == 64 => word((a as u32).rotate_right(immediate & 31)),
        (0x13, _, 5) if immediate == 0x287 => {
            let mut value = 0;
            for byte in 0..bits / 8 {
                if a & (0xff << (byte * 8)) != 0 {
                    value |= 0xff << (byte * 8);
                }
            }
            value
        }
        (0x13, _, 5) if (bits == 64 && immediate == 0x6b8) || (bits == 32 && immediate == 0x698) => {
            a.swap_bytes() >> (64 - bits)
        }
        (0x33, 0x24, 1) => a & !(1 << shift),
        (0x33, 0x24, 5) => (a >> shift) & 1,
        (0x33, 0x34, 1) => a ^ (1 << shift),
        (0x33, 0x14, 1) => a | (1 << shift),
        (0x13, _, 1 | 5) if matches!(insn >> 26, 0x12 | 0x1a | 0x0a) && (bits == 64 || immediate & 32 == 0) => {
            let index = immediate & (bits - 1);
            match (insn >> 26, funct) {
                (0x12, 1) => a & !(1 << index),
                (0x12, 5) => (a >> index) & 1,
                (0x1a, 1) => a ^ (1 << index),
                (0x0a, 1) => a | (1 << index),
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(result & mask)
}
