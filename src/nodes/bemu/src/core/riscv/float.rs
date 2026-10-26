use crate::core::riscv::{
    bus::{Bus, Width},
    csr::FS,
    hart::Hart,
    mmu::Mmu,
    Access, Trap,
};
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, FloatConvert, Round, Status, StatusAnd,
};

impl Hart {
    fn single_bits(&self, index: usize) -> u64 {
        let bits = self.f[index];
        if bits >> 32 == u32::MAX as u64 {
            bits as u32 as u64
        } else {
            0x7fc00000
        }
    }

    fn single(&self, index: usize) -> Single {
        Single::from_bits(self.single_bits(index).into())
    }

    fn rounding(&self, insn: u32) -> Result<Round, Trap> {
        let mut mode = (insn >> 12) & 7;
        if mode == 7 {
            mode = u32::from(self.csrs.fcsr >> 5);
        }
        match mode {
            0 => Ok(Round::NearestTiesToEven),
            1 => Ok(Round::TowardZero),
            2 => Ok(Round::TowardNegative),
            3 => Ok(Round::TowardPositive),
            4 => Ok(Round::NearestTiesToAway),
            _ => Err(Trap::illegal(insn)),
        }
    }

    pub(crate) fn execute_float(&mut self, bus: &mut impl Bus, mmu: &Mmu, insn: u32) -> Result<(), Trap> {
        let opcode = insn & 0x7f;
        let rd = ((insn >> 7) & 31) as usize;
        let rs1 = ((insn >> 15) & 31) as usize;
        let rs2 = ((insn >> 20) & 31) as usize;
        let rs3 = (insn >> 27) as usize;
        let funct = (insn >> 12) & 7;
        let op = insn >> 25;
        let illegal = Trap::illegal(insn);
        if opcode == 0x07 {
            let address = self.x[rs1].wrapping_add(((insn as i32) >> 20) as u64);
            self.f[rd] = match funct {
                2 => self.load(bus, mmu, address, Width::Word, Access::Load)? | 0xffffffff00000000,
                3 => self.load(bus, mmu, address, Width::Double, Access::Load)?,
                _ => return Err(illegal),
            };
            self.csrs.status |= FS;
            return Ok(());
        }
        if opcode == 0x27 {
            let offset = (((insn >> 7) & 31) | ((insn >> 20) & 0xfe0)) as i32;
            let address = self.x[rs1].wrapping_add(((offset << 20) >> 20) as u64);
            let width = match funct {
                2 => Width::Word,
                3 => Width::Double,
                _ => return Err(illegal),
            };
            return self.store(bus, mmu, address, width, self.f[rs2]);
        }
        let a = || self.single(rs1);
        let b = || self.single(rs2);
        let da = || Double::from_bits(self.f[rs1].into());
        let db = || Double::from_bits(self.f[rs2].into());
        let mut integer = false;
        let single;
        let result;
        if matches!(opcode, 0x43 | 0x47 | 0x4b | 0x4f) {
            let rm = self.rounding(insn)?;
            match (insn >> 25) & 3 {
                0 => {
                    let factor = if opcode == 0x4b || opcode == 0x4f { -a() } else { a() };
                    let c = self.single(rs3);
                    let c = if opcode == 0x47 || opcode == 0x4f { -c } else { c };
                    result = factor.mul_add_r(b(), c, rm).map(|v| v.to_bits() as u64);
                    single = true;
                }
                1 => {
                    let factor = if opcode == 0x4b || opcode == 0x4f { -da() } else { da() };
                    let c = Double::from_bits(self.f[rs3].into());
                    let c = if opcode == 0x47 || opcode == 0x4f { -c } else { c };
                    result = factor.mul_add_r(db(), c, rm).map(|v| v.to_bits() as u64);
                    single = false;
                }
                _ => return Err(illegal),
            }
        } else {
            single = op & 1 == 0;
            result = match op {
                0x00 => a().add_r(b(), self.rounding(insn)?).map(|v| v.to_bits() as u64),
                0x01 => da().add_r(db(), self.rounding(insn)?).map(|v| v.to_bits() as u64),
                0x04 => a().sub_r(b(), self.rounding(insn)?).map(|v| v.to_bits() as u64),
                0x05 => da().sub_r(db(), self.rounding(insn)?).map(|v| v.to_bits() as u64),
                0x08 => a().mul_r(b(), self.rounding(insn)?).map(|v| v.to_bits() as u64),
                0x09 => da().mul_r(db(), self.rounding(insn)?).map(|v| v.to_bits() as u64),
                0x0c => a().div_r(b(), self.rounding(insn)?).map(|v| v.to_bits() as u64),
                0x0d => da().div_r(db(), self.rounding(insn)?).map(|v| v.to_bits() as u64),
                0x2c if rs2 == 0 => sqrt(a().to_bits() as u64, true, self.rounding(insn)?),
                0x2d if rs2 == 0 => sqrt(da().to_bits() as u64, false, self.rounding(insn)?),
                0x10 => {
                    let a = self.single_bits(rs1);
                    let b = self.single_bits(rs2);
                    let sign = match funct {
                        0 => b,
                        1 => !b,
                        2 => a ^ b,
                        _ => return Err(illegal),
                    };
                    Status::OK.and((a & 0x7fffffff) | (sign & 0x80000000))
                }
                0x11 => {
                    let a = self.f[rs1];
                    let b = self.f[rs2];
                    let sign = match funct {
                        0 => b,
                        1 => !b,
                        2 => a ^ b,
                        _ => return Err(illegal),
                    };
                    Status::OK.and((a & 0x7fffffffffffffff) | (sign & 0x8000000000000000))
                }
                0x14 if funct <= 1 => {
                    let a = a();
                    let b = b();
                    let status = if a.is_signaling() || b.is_signaling() {
                        Status::INVALID_OP
                    } else {
                        Status::OK
                    };
                    let value = if a.is_nan() {
                        b
                    } else if b.is_nan() {
                        a
                    } else if funct == 0 {
                        a.minimum(b)
                    } else {
                        a.maximum(b)
                    };
                    status.and(value.to_bits() as u64)
                }
                0x15 if funct <= 1 => {
                    let da = da();
                    let db = db();
                    let status = if da.is_signaling() || db.is_signaling() {
                        Status::INVALID_OP
                    } else {
                        Status::OK
                    };
                    let value = if da.is_nan() {
                        db
                    } else if db.is_nan() {
                        da
                    } else if funct == 0 {
                        da.minimum(db)
                    } else {
                        da.maximum(db)
                    };
                    status.and(value.to_bits() as u64)
                }
                0x20 if rs2 == 1 => {
                    let converted: StatusAnd<Single> = da().convert_r(self.rounding(insn)?, &mut false);
                    converted.map(|v| v.to_bits() as u64)
                }
                0x21 if rs2 == 0 => {
                    let converted: StatusAnd<Double> = a().convert_r(self.rounding(insn)?, &mut false);
                    converted.map(|v| v.to_bits() as u64)
                }
                0x50 if funct <= 2 => {
                    let a = a();
                    let b = b();
                    integer = true;
                    let invalid = a.is_signaling() || b.is_signaling() || (funct != 2 && (a.is_nan() || b.is_nan()));
                    let value = match funct {
                        0 => a <= b,
                        1 => a < b,
                        2 => a == b,
                        _ => unreachable!(),
                    };
                    (if invalid { Status::INVALID_OP } else { Status::OK }).and(u64::from(value))
                }
                0x51 if funct <= 2 => {
                    let da = da();
                    let db = db();
                    integer = true;
                    let invalid =
                        da.is_signaling() || db.is_signaling() || (funct != 2 && (da.is_nan() || db.is_nan()));
                    let value = match funct {
                        0 => da <= db,
                        1 => da < db,
                        2 => da == db,
                        _ => unreachable!(),
                    };
                    (if invalid { Status::INVALID_OP } else { Status::OK }).and(u64::from(value))
                }
                0x60 if rs2 <= 3 => {
                    let a = a();
                    integer = true;
                    let rm = self.rounding(insn)?;
                    let width = if rs2 < 2 { 32 } else { 64 };
                    let mut converted = if rs2 & 1 == 0 {
                        a.to_i128_r(width, rm, &mut false).map(|v| v as u64)
                    } else {
                        a.to_u128_r(width, rm, &mut false).map(|v| v as u64)
                    };
                    if converted.status.contains(Status::INVALID_OP) {
                        converted.value = integer_limit(width, rs2 & 1 == 0, a.is_negative() && !a.is_nan());
                    }
                    if width == 32 {
                        converted.value = converted.value as i32 as u64;
                    }
                    converted
                }
                0x61 if rs2 <= 3 => {
                    let da = da();
                    integer = true;
                    let rm = self.rounding(insn)?;
                    let width = if rs2 < 2 { 32 } else { 64 };
                    let mut converted = if rs2 & 1 == 0 {
                        da.to_i128_r(width, rm, &mut false).map(|v| v as u64)
                    } else {
                        da.to_u128_r(width, rm, &mut false).map(|v| v as u64)
                    };
                    if converted.status.contains(Status::INVALID_OP) {
                        converted.value = integer_limit(width, rs2 & 1 == 0, da.is_negative() && !da.is_nan());
                    }
                    if width == 32 {
                        converted.value = converted.value as i32 as u64;
                    }
                    converted
                }
                0x68 => {
                    let rm = self.rounding(insn)?;
                    match rs2 {
                        0 => Single::from_i128_r(self.x[rs1] as i32 as i128, rm),
                        1 => Single::from_u128_r(self.x[rs1] as u32 as u128, rm),
                        2 => Single::from_i128_r(self.x[rs1] as i64 as i128, rm),
                        3 => Single::from_u128_r(self.x[rs1] as u128, rm),
                        _ => return Err(illegal),
                    }
                    .map(|v| v.to_bits() as u64)
                }
                0x69 => {
                    let rm = self.rounding(insn)?;
                    match rs2 {
                        0 => Double::from_i128_r(self.x[rs1] as i32 as i128, rm),
                        1 => Double::from_u128_r(self.x[rs1] as u32 as u128, rm),
                        2 => Double::from_i128_r(self.x[rs1] as i64 as i128, rm),
                        3 => Double::from_u128_r(self.x[rs1] as u128, rm),
                        _ => return Err(illegal),
                    }
                    .map(|v| v.to_bits() as u64)
                }
                0x70 if rs2 == 0 => {
                    integer = true;
                    Status::OK.and(match funct {
                        0 => self.f[rs1] as i32 as u64,
                        1 => classify(self.single_bits(rs1), true),
                        _ => return Err(illegal),
                    })
                }
                0x71 if rs2 == 0 => {
                    integer = true;
                    Status::OK.and(match funct {
                        0 => self.f[rs1],
                        1 => classify(self.f[rs1], false),
                        _ => return Err(illegal),
                    })
                }
                0x78 if rs2 == 0 && funct == 0 => Status::OK.and(u64::from(self.x[rs1] as u32)),
                0x79 if rs2 == 0 && funct == 0 => Status::OK.and(self.x[rs1]),
                _ => return Err(illegal),
            };
        }
        let flags = result.status;
        self.csrs.fcsr |= (u8::from(flags.contains(Status::INVALID_OP)) << 4)
            | (u8::from(flags.contains(Status::DIV_BY_ZERO)) << 3)
            | (u8::from(flags.contains(Status::OVERFLOW)) << 2)
            | (u8::from(flags.contains(Status::UNDERFLOW)) << 1)
            | u8::from(flags.contains(Status::INEXACT));
        self.csrs.status |= FS;
        let mut value = result.value;
        if integer {
            self.set_register(rd, value);
        } else {
            // Arithmetic returns the RISC-V canonical NaN; sign injection and moves preserve payloads.
            if opcode != 0x53 || !matches!(op, 0x10 | 0x11 | 0x78 | 0x79) {
                if single && value & 0x7f800000 == 0x7f800000 && value & 0x007fffff != 0 {
                    value = 0x7fc00000;
                }
                if !single && value & 0x7ff0000000000000 == 0x7ff0000000000000 && value & 0x000fffffffffffff != 0 {
                    value = 0x7ff8000000000000;
                }
            }
            self.f[rd] = if single { value | 0xffffffff00000000 } else { value };
        }
        Ok(())
    }
}

fn integer_limit(width: usize, signed: bool, negative: bool) -> u64 {
    match (signed, negative) {
        (true, true) => (-(1i128 << (width - 1))) as u64,
        (true, false) => ((1u128 << (width - 1)) - 1) as u64,
        (false, true) => 0,
        (false, false) => ((1u128 << width) - 1) as u64,
    }
}

fn classify(bits: u64, single: bool) -> u64 {
    let (fraction_bits, exponent_mask) = if single { (23, 0xff) } else { (52, 0x7ff) };
    let fraction = bits & ((1 << fraction_bits) - 1);
    let exponent = (bits >> fraction_bits) & exponent_mask;
    let negative = bits >> if single { 31 } else { 63 } != 0;
    let index = if exponent == exponent_mask {
        if fraction == 0 {
            if negative {
                0
            } else {
                7
            }
        } else if fraction & (1 << (fraction_bits - 1)) == 0 {
            8
        } else {
            9
        }
    } else if exponent == 0 {
        if fraction == 0 {
            if negative {
                3
            } else {
                4
            }
        } else if negative {
            2
        } else {
            5
        }
    } else if negative {
        1
    } else {
        6
    };
    1 << index
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
