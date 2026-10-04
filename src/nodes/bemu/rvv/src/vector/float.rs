use crate::{Engine, Fault};
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, Round, Status, StatusAnd,
};

fn calculate<T: Float>(a: u64, b: u64, old: u64, op: u32, conversion: usize, bits: usize, rm: Round) -> StatusAnd<u64> {
    if op == 0x13 {
        return sqrt(a, bits == 32, rm);
    }
    let arithmetic = match op {
        0 => Some((a, b, old, 0)),
        2 => Some((a, b, old, 4)),
        0x20 => Some((a, b, old, 12)),
        0x21 => Some((b, a, old, 12)),
        0x24 => Some((a, b, old, 8)),
        0x27 => Some((b, a, old, 4)),
        0x28 => Some((b, old, a, 0x43)),
        0x29 => Some((b, old, a, 0x4f)),
        0x2a => Some((b, old, a, 0x47)),
        0x2b => Some((b, old, a, 0x4b)),
        0x2c => Some((b, a, old, 0x43)),
        0x2d => Some((b, a, old, 0x4f)),
        0x2e => Some((b, a, old, 0x47)),
        0x2f => Some((b, a, old, 0x4b)),
        _ => None,
    };
    if let Some((a, b, c, operation)) = arithmetic {
        return crate::float::arithmetic(a, b, c, operation, bits, rm);
    }
    let a = T::from_bits(a.into());
    let b = if op == 0x12 { T::ZERO } else { T::from_bits(b.into()) };
    match op {
        4 | 6 => {
            let flags = if a.is_signaling() || b.is_signaling() {
                Status::INVALID_OP
            } else {
                Status::OK
            };
            flags.and(
                if a.is_nan() {
                    b
                } else if b.is_nan() {
                    a
                } else if op == 4 {
                    a.minimum(b)
                } else {
                    a.maximum(b)
                }
                .to_bits() as u64,
            )
        }
        0x18 | 0x19 | 0x1b | 0x1c | 0x1d | 0x1f => {
            let flags =
                if a.is_signaling() || b.is_signaling() || (!matches!(op, 0x18 | 0x1c) && (a.is_nan() || b.is_nan())) {
                    Status::INVALID_OP
                } else {
                    Status::OK
                };
            flags.and(u64::from(match op {
                0x18 => a == b,
                0x19 => a <= b,
                0x1b => a < b,
                0x1c => a != b,
                0x1d => a > b,
                0x1f => a >= b,
                _ => unreachable!(),
            }))
        }
        0x12 if matches!(conversion, 0 | 1 | 6 | 7) => {
            let signed = conversion & 1 != 0;
            let rounding = if conversion >= 6 { Round::TowardZero } else { rm };
            let mut result = if signed {
                a.to_i128_r(bits, rounding, &mut false).map(|v| v as u64)
            } else {
                a.to_u128_r(bits, rounding, &mut false).map(|v| v as u64)
            };
            if result.status.contains(Status::INVALID_OP) {
                let negative = a.is_negative() && !a.is_nan();
                result.value = match (signed, negative) {
                    (true, true) => (-(1i128 << (bits - 1))) as u64,
                    (true, false) => ((1u128 << (bits - 1)) - 1) as u64,
                    (false, true) => 0,
                    (false, false) => u64::MAX >> (64 - bits),
                };
            }
            result
        }
        _ => unreachable!(),
    }
}

fn sqrt(bits: u64, single: bool, round: Round) -> StatusAnd<u64> {
    let (fraction_bits, exponent_mask, bias) = if single { (23, 0xff, 127) } else { (52, 0x7ff, 1023) };
    let fraction = bits & ((1 << fraction_bits) - 1);
    let exponent = (bits >> fraction_bits) & exponent_mask;
    let negative = bits >> if single { 31 } else { 63 } != 0;
    let nan = if single { 0x7fc00000 } else { 0x7ff8000000000000 };
    if exponent == exponent_mask && fraction != 0 {
        let status = if fraction & (1 << (fraction_bits - 1)) == 0 {
            Status::INVALID_OP
        } else {
            Status::OK
        };
        return status.and(nan);
    }
    if exponent == 0 && fraction == 0 {
        return Status::OK.and(bits);
    }
    if negative {
        return Status::INVALID_OP.and(nan);
    }
    if exponent == exponent_mask {
        return Status::OK.and(bits);
    }
    let mut significand = fraction;
    let mut power = exponent as i32 - bias;
    if exponent == 0 {
        let shift = fraction.leading_zeros() - (63 - fraction_bits);
        significand <<= shift;
        power = 1 - bias - shift as i32;
    } else {
        significand |= 1 << fraction_bits;
    }
    if power & 1 != 0 {
        significand <<= 1;
        power -= 1;
    }
    let radicand = (significand as u128) << fraction_bits;
    let mut root = radicand.isqrt();
    let remainder = radicand - root * root;
    if remainder != 0
        && match round {
            Round::TowardPositive => true,
            Round::TowardZero | Round::TowardNegative => false,
            // (root + 1/2)^2 is non-integral, so a midpoint tie is impossible.
            Round::NearestTiesToEven | Round::NearestTiesToAway => remainder > root,
        }
    {
        root += 1;
    }
    let mut encoded_exponent = power / 2 + bias;
    if root == 1 << (fraction_bits + 1) {
        root >>= 1;
        encoded_exponent += 1;
    }
    let value = (encoded_exponent as u64) << fraction_bits | (root as u64 & ((1 << fraction_bits) - 1));
    (if remainder == 0 { Status::OK } else { Status::INEXACT }).and(value)
}


impl Engine {
    pub(crate) fn vector_float(&mut self) -> Result<(), Fault> {
        let insn = self.instruction;
        let illegal = self.illegal();
        let vd = ((insn >> 7) & 31) as usize;
        let kind = (insn >> 12) & 7;
        let vs1 = ((insn >> 15) & 31) as usize;
        let vs2 = ((insn >> 20) & 31) as usize;
        let masked = insn & (1 << 25) == 0;
        let op = insn >> 26;
        let vector = &self.vector;
        let bits = vector.sew();
        if op == 0x10 && kind == 1 && vs1 == 0 && !masked && matches!(bits, 32 | 64) {
            let value = vector.read(vs2, 0, bits);
            self.f[vd] = if bits == 32 { value | 0xffffffff00000000 } else { value };
            return Ok(());
        }
        if op == 0x10 && kind == 5 {
            if masked || vs2 != 0 || !matches!(bits, 32 | 64) {
                return Err(illegal);
            }
            if vector.vstart < vector.vl {
                let value = if bits == 32 && self.f[vs1] >> 32 != u32::MAX as u64 {
                    0x7fc00000
                } else {
                    self.f[vs1]
                };
                self.vector.write(vd, 0, bits, value);
            }
            return Ok(());
        }
        if kind == 1 && matches!(op, 3 | 7) {
            if !matches!(bits, 32 | 64) || vector.vstart != 0 || !vector.group(vs2, bits) {
                return Err(illegal);
            }
            let rm = match self.fcsr >> 5 {
                0 => Round::NearestTiesToEven,
                1 => Round::TowardZero,
                2 => Round::TowardNegative,
                3 => Round::TowardPositive,
                4 => Round::NearestTiesToAway,
                _ => return Err(illegal),
            };
            if vector.vl == 0 {
                return Ok(());
            }
            let mut value = vector.read(vs1, 0, bits);
            let mut flags = Status::OK;
            let operation = if op == 3 { 0 } else { 6 };
            for index in 0..vector.vl {
                if masked && !vector.mask(0, index) {
                    continue;
                }
                let input = vector.read(vs2, index, bits);
                let result = if bits == 32 {
                    calculate::<Single>(value, input, 0, operation, 0, bits, rm)
                } else {
                    calculate::<Double>(value, input, 0, operation, 0, bits, rm)
                };
                flags |= result.status;
                value = result.value;
                if bits == 32 && (value as u32 & 0x7fffffff) > 0x7f800000 {
                    value = 0x7fc00000;
                }
                if bits == 64 && (value & 0x7fffffffffffffff) > 0x7ff0000000000000 {
                    value = 0x7ff8000000000000;
                }
            }
            self.fcsr |= (u8::from(flags.contains(Status::INVALID_OP)) << 4)
                | (u8::from(flags.contains(Status::DIV_BY_ZERO)) << 3)
                | (u8::from(flags.contains(Status::OVERFLOW)) << 2)
                | (u8::from(flags.contains(Status::UNDERFLOW)) << 1)
                | u8::from(flags.contains(Status::INEXACT));
            self.vector.write(vd, 0, bits, value);
            return Ok(());
        }

        let compare = matches!(op, 0x18 | 0x19 | 0x1b | 0x1c | 0x1d | 0x1f);
        let merge = op == 0x17;
        let source_bits = if op == 0x12 && (16..=23).contains(&vs1) {
            bits * 2
        } else {
            bits
        };
        let destination_bits = if op == 0x12 && (8..=15).contains(&vs1) {
            bits * 2
        } else {
            bits
        };
        if !matches!(bits, 32 | 64)
            || source_bits > 64
            || destination_bits > 64
            || !vector.group(vs2, source_bits)
            || (!compare && !vector.group(vd, destination_bits))
            || (kind == 1 && op != 0x12 && !vector.group(vs1, bits))
            || (masked && vd == 0 && !compare)
            || (merge && (kind != 5 || (!masked && vs2 != 0)))
            || (op == 0x12 && (kind != 1 || !matches!(vs1, 0..=3 | 6..=12 | 14..=20 | 22 | 23)))
            || (op == 0x13 && (kind != 1 || vs1 != 0))
            || !matches!(op, 0 | 2 | 4 | 6 | 8..=10 | 0x12 | 0x13 | 0x17 | 0x18 | 0x19 | 0x1b..=0x1d | 0x1f..=0x21 | 0x24 | 0x27..=0x2f)
        {
            return Err(illegal);
        }
        let rm = match self.fcsr >> 5 {
            0 => Round::NearestTiesToEven,
            1 => Round::TowardZero,
            2 => Round::TowardNegative,
            3 => Round::TowardPositive,
            4 => Round::NearestTiesToAway,
            _ => return Err(illegal),
        };
        let scalar = if bits == 32 {
            if self.f[vs1] >> 32 == u32::MAX as u64 {
                self.f[vs1] & 0xffffffff
            } else {
                0x7fc00000
            }
        } else {
            self.f[vs1]
        };
        let start = vector.vstart;
        let length = vector.vl;
        for index in start..length {
            let vector = &mut self.vector;
            let selected = !masked || vector.mask(0, index);
            if masked && !selected && !merge {
                continue;
            }
            let a = vector.read(vs2, index, source_bits);
            let b = if kind == 1 && op != 0x12 {
                vector.read(vs1, index, bits)
            } else {
                scalar
            };
            let old = if matches!(op, 0x28..=0x2f) {
                vector.read(vd, index, bits)
            } else {
                0
            };
            let sign = 1u64 << (bits - 1);
            let result = if merge {
                Status::OK.and(if !masked || selected { b } else { a })
            } else if matches!(op, 8..=10) {
                let sign_value = match op {
                    8 => b,
                    9 => !b,
                    10 => a ^ b,
                    _ => unreachable!(),
                };
                Status::OK.and((a & !sign) | (sign_value & sign))
            } else if op == 0x12 && matches!(vs1, 12 | 20) {
                crate::float::convert(a, source_bits, rm)
            } else if op == 0x12 && matches!(vs1 & 7, 2 | 3) {
                let signed = vs1 & 7 == 3;
                let signed_value = if source_bits == 32 {
                    a as i32 as i128
                } else {
                    a as i64 as i128
                };
                if destination_bits == 64 {
                    if signed {
                        Double::from_i128_r(signed_value, rm)
                    } else {
                        Double::from_u128_r(a.into(), rm)
                    }
                    .map(|v| v.to_bits() as u64)
                } else {
                    if signed {
                        Single::from_i128_r(signed_value, rm)
                    } else {
                        Single::from_u128_r(a.into(), rm)
                    }
                    .map(|v| v.to_bits() as u64)
                }
            } else {
                let conversion = vs1 & 7;
                if source_bits == 64 {
                    calculate::<Double>(a, b, old, op, conversion, destination_bits, rm)
                } else {
                    calculate::<Single>(a, b, old, op, conversion, destination_bits, rm)
                }
            };
            let flags = result.status;
            self.fcsr |= (u8::from(flags.contains(Status::INVALID_OP)) << 4)
                | (u8::from(flags.contains(Status::DIV_BY_ZERO)) << 3)
                | (u8::from(flags.contains(Status::OVERFLOW)) << 2)
                | (u8::from(flags.contains(Status::UNDERFLOW)) << 1)
                | u8::from(flags.contains(Status::INEXACT));
            let vector = &mut self.vector;
            if compare {
                vector.write_mask(vd, index, result.value != 0);
            } else {
                let integer_result = op == 0x12 && matches!(vs1 & 7, 0 | 1 | 6 | 7);
                let preserve = merge || matches!(op, 8..=10) || integer_result;
                let mut value = result.value;
                if !preserve {
                    if destination_bits == 32 && (value as u32 & 0x7fffffff) > 0x7f800000 {
                        value = 0x7fc00000;
                    }
                    if destination_bits == 64 && (value & 0x7fffffffffffffff) > 0x7ff0000000000000 {
                        value = 0x7ff8000000000000;
                    }
                }
                vector.write(vd, index, destination_bits, value);
            }
        }
        Ok(())
    }
}
