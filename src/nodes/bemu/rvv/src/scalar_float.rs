use crate::{Engine, Fault};
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, Round, Status, StatusAnd,
};

impl Engine {
    pub(crate) fn scalar_float(&mut self) -> Result<(), Fault> {
        let insn = self.instruction;
        let opcode = insn & 0x7f;
        let rd = ((insn >> 7) & 31) as usize;
        let funct = (insn >> 12) & 7;
        let rs1 = ((insn >> 15) & 31) as usize;
        let rs2 = ((insn >> 20) & 31) as usize;
        let op = insn >> 25;
        let format = if opcode == 0x53 { op & 3 } else { (insn >> 25) & 3 };
        if format > 1 || (format == 1 && self.vector.elen < 64) {
            return Err(self.illegal());
        }
        let double = format == 1;
        let single = |bits: u64| {
            if bits >> 32 == u32::MAX as u64 {
                bits & 0xffffffff
            } else {
                0x7fc00000
            }
        };
        let a = if double { self.f[rs1] } else { single(self.f[rs1]) };
        let b = if double { self.f[rs2] } else { single(self.f[rs2]) };
        let rs3 = (insn >> 27) as usize;
        let c = if double { self.f[rs3] } else { single(self.f[rs3]) };
        let bits = if double { 64 } else { 32 };
        let sign = 1u64 << (bits - 1);
        let operation = op & !3;
        if self.vector.elen < 64 && (operation == 0x20 || (matches!(operation, 0x60 | 0x68) && rs2 >= 2)) {
            return Err(self.illegal());
        }
        let uses_rounding = opcode != 0x53 || matches!(operation, 0 | 4 | 8 | 12 | 0x20 | 0x2c | 0x60 | 0x68);
        let rm = if uses_rounding {
            match if funct == 7 { self.fcsr >> 5 } else { funct as u8 } {
                0 => Round::NearestTiesToEven,
                1 => Round::TowardZero,
                2 => Round::TowardNegative,
                3 => Round::TowardPositive,
                4 => Round::NearestTiesToAway,
                _ => return Err(self.illegal()),
            }
        } else {
            Round::NearestTiesToEven
        };
        let mut integer = false;
        let mut preserve = false;
        let result = if opcode != 0x53 || matches!(operation, 0 | 4 | 8 | 12) {
            let operation = if opcode != 0x53 { opcode } else { operation };
            crate::float::arithmetic(a, b, c, operation, bits, rm)
        } else {
            match operation {
                0x10 if funct <= 2 => {
                    preserve = true;
                    let value = match funct {
                        0 => b,
                        1 => !b,
                        2 => a ^ b,
                        _ => unreachable!(),
                    };
                    Status::OK.and((a & !sign) | (value & sign))
                }
                0x14 if funct <= 1 => {
                    fn minmax<T: Float>(a: u64, b: u64, maximum: bool) -> StatusAnd<u64> {
                        let a = T::from_bits(a.into());
                        let b = T::from_bits(b.into());
                        let flags = if a.is_signaling() || b.is_signaling() { Status::INVALID_OP } else { Status::OK };
                        flags.and(if a.is_nan() { b } else if b.is_nan() { a }
                                  else if maximum { a.maximum(b) } else { a.minimum(b) }.to_bits() as u64)
                    }
                    if double { minmax::<Double>(a, b, funct == 1) } else { minmax::<Single>(a, b, funct == 1) }
                }
                0x2c if rs2 == 0 => crate::vector::sqrt(a, !double, rm),
                0x70 if rs2 == 0 && funct == 1 => {
                    integer = true;
                    let fraction_bits = if double { 52 } else { 23 };
                    let exponent_mask = if double { 0x7ff } else { 0xff };
                    let fraction = a & ((1u64 << fraction_bits) - 1);
                    let exponent = (a >> fraction_bits) & exponent_mask;
                    let negative = a & sign != 0;
                    let class = if exponent == exponent_mask {
                        if fraction == 0 { if negative { 0 } else { 7 } }
                        else if fraction & (1 << (fraction_bits - 1)) == 0 { 8 } else { 9 }
                    } else if exponent == 0 {
                        if fraction == 0 { if negative { 3 } else { 4 } }
                        else if negative { 2 } else { 5 }
                    } else if negative { 1 } else { 6 };
                    Status::OK.and(1 << class)
                }
                0x20 if (!double && rs2 == 1) || (double && rs2 == 0) => crate::float::convert(
                    if double { single(self.f[rs1]) } else { self.f[rs1] },
                    if double { 32 } else { 64 },
                    rm,
                ),
                0x50 if funct <= 2 => {
                    integer = true;
                    let (nan, signaling, eq, lt, le) = if double {
                        let a = Double::from_bits(a.into());
                        let b = Double::from_bits(b.into());
                        (
                            a.is_nan() || b.is_nan(),
                            a.is_signaling() || b.is_signaling(),
                            a == b,
                            a < b,
                            a <= b,
                        )
                    } else {
                        let a = Single::from_bits(a.into());
                        let b = Single::from_bits(b.into());
                        (
                            a.is_nan() || b.is_nan(),
                            a.is_signaling() || b.is_signaling(),
                            a == b,
                            a < b,
                            a <= b,
                        )
                    };
                    let status = if signaling || (funct != 2 && nan) {
                        Status::INVALID_OP
                    } else {
                        Status::OK
                    };
                    status.and(u64::from(match funct {
                        0 => le,
                        1 => lt,
                        2 => eq,
                        _ => unreachable!(),
                    }))
                }
                0x60 if rs2 <= 3 => {
                    integer = true;
                    let signed = rs2 & 1 == 0;
                    let width = if rs2 < 2 { 32 } else { 64 };
                    let mut result = if double {
                        let a = Double::from_bits(a.into());
                        if signed {
                            a.to_i128_r(width, rm, &mut false).map(|v| v as u64)
                        } else {
                            a.to_u128_r(width, rm, &mut false).map(|v| v as u64)
                        }
                    } else {
                        let a = Single::from_bits(a.into());
                        if signed {
                            a.to_i128_r(width, rm, &mut false).map(|v| v as u64)
                        } else {
                            a.to_u128_r(width, rm, &mut false).map(|v| v as u64)
                        }
                    };
                    if result.status.contains(Status::INVALID_OP) {
                        let negative = a & sign != 0
                            && if double {
                                !Double::from_bits(a.into()).is_nan()
                            } else {
                                !Single::from_bits(a.into()).is_nan()
                            };
                        result.value = match (signed, negative) {
                            (true, true) => 1u64 << (width - 1),
                            (true, false) => (1u64 << (width - 1)) - 1,
                            (false, true) => 0,
                            (false, false) => u64::MAX >> (64 - width),
                        };
                    }
                    if width == 32 {
                        result.value = result.value as i32 as i64 as u64;
                    }
                    result
                }
                0x68 if rs2 <= 3 => {
                    if double {
                        match rs2 {
                            0 => Double::from_i128_r(self.x[rs1] as i32 as i128, rm),
                            1 => Double::from_u128_r(self.x[rs1] as u32 as u128, rm),
                            2 => Double::from_i128_r(self.x[rs1] as i64 as i128, rm),
                            3 => Double::from_u128_r(self.x[rs1] as u128, rm),
                            _ => unreachable!(),
                        }
                        .map(|v| v.to_bits() as u64)
                    } else {
                        match rs2 {
                            0 => Single::from_i128_r(self.x[rs1] as i32 as i128, rm),
                            1 => Single::from_u128_r(self.x[rs1] as u32 as u128, rm),
                            2 => Single::from_i128_r(self.x[rs1] as i64 as i128, rm),
                            3 => Single::from_u128_r(self.x[rs1] as u128, rm),
                            _ => unreachable!(),
                        }
                        .map(|v| v.to_bits() as u64)
                    }
                }
                0x70 if rs2 == 0 && funct == 0 => {
                    integer = true;
                    Status::OK.and(if double { self.f[rs1] } else { self.f[rs1] as i32 as i64 as u64 })
                }
                0x78 if rs2 == 0 && funct == 0 => {
                    preserve = true;
                    Status::OK.and(if double { self.x[rs1] } else { self.x[rs1] & 0xffffffff })
                }
                _ => return Err(self.illegal()),
            }
        };
        let flags = result.status;
        self.fcsr |= (u8::from(flags.contains(Status::INVALID_OP)) << 4)
            | (u8::from(flags.contains(Status::DIV_BY_ZERO)) << 3)
            | (u8::from(flags.contains(Status::OVERFLOW)) << 2)
            | (u8::from(flags.contains(Status::UNDERFLOW)) << 1)
            | u8::from(flags.contains(Status::INEXACT));
        if integer {
            self.x[rd] = result.value;
        } else {
            let value = if !preserve
                && if double {
                    Double::from_bits(result.value.into()).is_nan()
                } else {
                    Single::from_bits(result.value.into()).is_nan()
                } {
                if double {
                    0x7ff8000000000000
                } else {
                    0x7fc00000
                }
            } else {
                result.value
            };
            self.f[rd] = if double { value } else { value | 0xffffffff00000000 };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Engine;
    fn instruction(op: u32, selector: u32) -> u32 {
        (op << 25) | (selector << 20) | (10 << 15) | (12 << 7) | 0x53
    }
    #[test]
    fn rv64_float_integer_width_and_moves() {
        let mut engine = Engine::new(256, 64, 4096);
        engine.x[10] = 0x123456789abcdef0;
        engine.instruction = instruction(0x79, 0); // fmv.d.x
        engine.scalar_float().unwrap();
        assert_eq!(engine.f[12], 0x123456789abcdef0);
        engine.f[10] = engine.f[12];
        engine.instruction = instruction(0x71, 0); // fmv.x.d
        engine.scalar_float().unwrap();
        assert_eq!(engine.x[12], 0x123456789abcdef0);
        engine.f[10] = 0xffffffff80000000;
        engine.instruction = instruction(0x70, 0); // fmv.x.w
        engine.scalar_float().unwrap();
        assert_eq!(engine.x[12], 0xffffffff80000000);
        engine.x[10] = 0xabcdef017fc01234;
        engine.instruction = instruction(0x78, 0); // fmv.w.x
        engine.scalar_float().unwrap();
        assert_eq!(engine.f[12], 0xffffffff7fc01234);
    }
    #[test]
    fn rv64_float_converts_long_and_unsigned_word() {
        let mut engine = Engine::new(256, 64, 4096);
        engine.x[10] = (1u64 << 40) + 3;
        engine.instruction = instruction(0x69, 3); // fcvt.d.lu
        engine.scalar_float().unwrap();
        assert_eq!(engine.f[12], ((1u64 << 40) as f64 + 3.).to_bits());
        engine.f[10] = engine.f[12];
        engine.instruction = instruction(0x61, 3); // fcvt.lu.d
        engine.scalar_float().unwrap();
        assert_eq!(engine.x[12], (1u64 << 40) + 3);
        engine.x[10] = (-((1i64 << 40) + 3)) as u64;
        engine.instruction = instruction(0x69, 2); // fcvt.d.l
        engine.scalar_float().unwrap();
        assert_eq!(engine.f[12], (-((1u64 << 40) as f64 + 3.)).to_bits());
        engine.f[10] = engine.f[12];
        engine.instruction = instruction(0x61, 2); // fcvt.l.d
        engine.scalar_float().unwrap();
        assert_eq!(engine.x[12], (-((1i64 << 40) + 3)) as u64);
        engine.x[10] = 0x1234567880000000;
        engine.instruction = instruction(0x69, 1); // fcvt.d.wu ignores upper 32 bits
        engine.scalar_float().unwrap();
        assert_eq!(engine.f[12], (2147483648f64).to_bits());
        engine.f[10] = engine.f[12];
        engine.instruction = instruction(0x61, 1); // fcvt.wu.d sign-extends word result
        engine.scalar_float().unwrap();
        assert_eq!(engine.x[12], 0xffffffff80000000);
    }
    #[test]
    fn rv64_float_invalid_long_conversion_limits() {
        let mut engine = Engine::new(256, 64, 4096);
        for (value, selector, expected) in [
            (f64::NAN, 2, i64::MAX as u64),
            (f64::NEG_INFINITY, 2, i64::MIN as u64),
            (f64::INFINITY, 3, u64::MAX),
            (-1f64, 3, 0),
        ] {
            engine.f[10] = value.to_bits();
            engine.instruction = instruction(0x61, selector);
            engine.scalar_float().unwrap();
            assert_eq!(engine.x[12], expected);
            assert_ne!(engine.fcsr & 16, 0);
        }
    }
    #[test]
    fn scalar_float_sqrt_minmax_and_class() {
        let mut engine = Engine::new(256, 64, 4096);
        engine.f[10] = 9f64.to_bits();
        engine.instruction = instruction(0x2d, 0);
        engine.scalar_float().unwrap();
        assert_eq!(engine.f[12], 3f64.to_bits());
        engine.f[10] = (-1f64).to_bits();
        engine.scalar_float().unwrap();
        assert_eq!(engine.f[12], 0x7ff8000000000000);
        assert_ne!(engine.fcsr & 16, 0);
        engine.fcsr = 0;
        engine.f[10] = (-0f64).to_bits();
        engine.f[0] = 0f64.to_bits();
        engine.instruction = instruction(0x15, 0);
        engine.scalar_float().unwrap();
        assert_eq!(engine.f[12], (-0f64).to_bits());
        engine.instruction |= 1 << 12;
        engine.scalar_float().unwrap();
        assert_eq!(engine.f[12], 0f64.to_bits());
        for (value, expected) in [(f64::NEG_INFINITY.to_bits(), 1),
                                   ((-1f64).to_bits(), 2),
                                   ((-0f64).to_bits(), 8),
                                   (0f64.to_bits(), 16),
                                   (1, 32),
                                   (1f64.to_bits(), 64),
                                   (f64::INFINITY.to_bits(), 128),
                                   (0x7ff0000000000001, 256),
                                   (f64::NAN.to_bits(), 512)] {
            engine.f[10] = value;
            engine.instruction = instruction(0x71, 0) | (1 << 12);
            engine.scalar_float().unwrap();
            assert_eq!(engine.x[12], expected);
            assert_eq!(engine.fcsr, 0);
        }
    }

    #[test]
    fn elen32_float_rejects_double_and_long_formats() {
        for vlen in [128, 256, 1024] {
            let mut engine = Engine::new(vlen, 32, 4096);
            engine.f[10] = 0xffffffff00000000 | 1.5f32.to_bits() as u64;
            engine.instruction = instruction(0x60, 0);
            engine.scalar_float().unwrap();
            assert_eq!(engine.x[12], 2);
            for (op, selector) in [(1, 0), (0x61, 0), (0x60, 2), (0x68, 2), (0x20, 1), (0x21, 0), (0x71, 0), (0x79, 0)] {
                engine.instruction = instruction(op, selector);
                assert_eq!(engine.scalar_float().unwrap_err().cause, 2);
            }
        }
    }

}
