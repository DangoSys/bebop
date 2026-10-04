use crate::{Engine, Fault};
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, Round, Status,
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
        if format > 1 {
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
        let uses_rounding = opcode != 0x53 || matches!(operation, 0 | 4 | 8 | 12 | 0x20 | 0x60 | 0x68);
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
                0x60 if rs2 <= 1 => {
                    integer = true;
                    let signed = rs2 == 0;
                    let mut result = if double {
                        let a = Double::from_bits(a.into());
                        if signed {
                            a.to_i128_r(32, rm, &mut false).map(|v| v as u64)
                        } else {
                            a.to_u128_r(32, rm, &mut false).map(|v| v as u64)
                        }
                    } else {
                        let a = Single::from_bits(a.into());
                        if signed {
                            a.to_i128_r(32, rm, &mut false).map(|v| v as u64)
                        } else {
                            a.to_u128_r(32, rm, &mut false).map(|v| v as u64)
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
                            (true, true) => 0x80000000,
                            (true, false) => 0x7fffffff,
                            (false, true) => 0,
                            (false, false) => 0xffffffff,
                        };
                    }
                    result
                }
                0x68 if rs2 <= 1 => {
                    if double {
                        if rs2 == 0 {
                            Double::from_i128_r(self.x[rs1] as i32 as i128, rm)
                        } else {
                            Double::from_u128_r(self.x[rs1].into(), rm)
                        }
                        .map(|v| v.to_bits() as u64)
                    } else {
                        if rs2 == 0 {
                            Single::from_i128_r(self.x[rs1] as i32 as i128, rm)
                        } else {
                            Single::from_u128_r(self.x[rs1].into(), rm)
                        }
                        .map(|v| v.to_bits() as u64)
                    }
                }
                0x70 if rs2 == 0 && funct == 0 && !double => {
                    integer = true;
                    Status::OK.and(self.f[rs1] & 0xffffffff)
                }
                0x78 if rs2 == 0 && funct == 0 && !double => {
                    preserve = true;
                    Status::OK.and(self.x[rs1].into())
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
            self.x[rd] = result.value as u32;
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
