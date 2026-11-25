use crate::riscv::{
    bus::{Bus, Width},
    csr::FS,
    mmu::Mmu,
    registers::Registers,
    Access, Trap,
};
use rustc_apfloat::{
    ieee::{Double, Half, Single},
    Float, FloatConvert, Round, Status, StatusAnd,
};

impl Registers {
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

    pub fn execute_float(&mut self, bus: &mut impl Bus, mmu: &Mmu, insn: u32) -> Result<(), Trap> {
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
                1 if self.half_precision => {
                    self.load(bus, mmu, address, Width::Half, Access::Load)? | 0xffffffffffff0000
                }
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
                1 if self.half_precision => Width::Half,
                2 => Width::Word,
                3 => Width::Double,
                _ => return Err(illegal),
            };
            return self.store(bus, mmu, address, width, self.f[rs2]);
        }
        // Half precision, as far as a hart that feeds FP16 data to an accelerator has it: moves
        // and the conversions with single and double precision.
        if opcode == 0x53 && (matches!(op, 0x22 | 0x72 | 0x7a) || matches!(op, 0x20 | 0x21) && rs2 == 2) {
            if !self.half_precision {
                return Err(illegal);
            }
            // A value that is not boxed reads as the canonical NaN.
            let half = if self.f[rs1] >> 16 == 0xffff_ffff_ffff {
                Half::from_bits((self.f[rs1] & 0xffff).into())
            } else {
                Half::from_bits(0x7e00)
            };
            let status = match (op, rs2, funct) {
                (0x20, 2, _) => {
                    let converted: StatusAnd<Single> = half.convert_r(self.rounding(insn)?, &mut false);
                    let bits = if converted.value.is_nan() {
                        0x7fc00000
                    } else {
                        converted.value.to_bits() as u64
                    };
                    self.f[rd] = bits | 0xffffffff00000000;
                    converted.status
                }
                (0x21, 2, _) => {
                    let converted: StatusAnd<Double> = half.convert_r(self.rounding(insn)?, &mut false);
                    self.f[rd] = if converted.value.is_nan() {
                        0x7ff8000000000000
                    } else {
                        converted.value.to_bits() as u64
                    };
                    converted.status
                }
                (0x22, 0, _) => {
                    let converted: StatusAnd<Half> = self.single(rs1).convert_r(self.rounding(insn)?, &mut false);
                    let bits = if converted.value.is_nan() {
                        0x7e00
                    } else {
                        converted.value.to_bits() as u64
                    };
                    self.f[rd] = bits | 0xffffffffffff0000;
                    converted.status
                }
                (0x22, 1, _) => {
                    let converted: StatusAnd<Half> =
                        Double::from_bits(self.f[rs1].into()).convert_r(self.rounding(insn)?, &mut false);
                    let bits = if converted.value.is_nan() {
                        0x7e00
                    } else {
                        converted.value.to_bits() as u64
                    };
                    self.f[rd] = bits | 0xffffffffffff0000;
                    converted.status
                }
                (0x72, 0, 0) => {
                    self.set_register(rd, self.f[rs1] as i16 as u64);
                    Status::OK
                }
                (0x7a, 0, 0) => {
                    self.f[rd] = self.x[rs1] | 0xffffffffffff0000;
                    Status::OK
                }
                _ => return Err(illegal),
            };
            self.csrs.fcsr |= (u8::from(status.contains(Status::INVALID_OP)) << 4)
                | (u8::from(status.contains(Status::OVERFLOW)) << 2)
                | (u8::from(status.contains(Status::UNDERFLOW)) << 1)
                | u8::from(status.contains(Status::INEXACT));
            self.csrs.status |= FS;
            return Ok(());
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
                    let a = self.f[rs1] ^ if opcode == 0x4b || opcode == 0x4f { 1 << 63 } else { 0 };
                    let c = self.f[rs3] ^ if opcode == 0x47 || opcode == 0x4f { 1 << 63 } else { 0 };
                    result = binary64(a, self.f[rs2], c, 16, rm);
                    single = false;
                }
                _ => return Err(illegal),
            }
        } else {
            single = op & 1 == 0;
            result = match op {
                0x00 | 0x04 | 0x08 | 0x0c => {
                    binary32(self.single_bits(rs1), self.single_bits(rs2), op, self.rounding(insn)?)
                }
                0x01 | 0x05 | 0x09 | 0x0d => binary64(self.f[rs1], self.f[rs2], 0, op & !1, self.rounding(insn)?),
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
                0x20 if rs2 == 1 => convert_format(self.f[rs1], true, self.rounding(insn)?),
                0x21 if rs2 == 0 => convert_format(self.single_bits(rs1), false, self.rounding(insn)?),
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
                0x68 | 0x69 if rs2 < 4 => from_integer(self.x[rs1], rs2, op == 0x68, self.rounding(insn)?),
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

#[inline]
fn from_integer(bits: u64, kind: usize, single: bool, round: Round) -> StatusAnd<u64> {
    let bits = match kind {
        0 => bits as i32 as i64 as u64,
        1 => bits as u32 as u64,
        2 | 3 => bits,
        _ => unreachable!(),
    };
    let signed = kind & 1 == 0;
    let magnitude = if signed { (bits as i64).unsigned_abs() } else { bits };
    let precision = if single { 24 } else { 53 };
    let inexact = magnitude != 0 && 64 - magnitude.leading_zeros() - magnitude.trailing_zeros() > precision;
    // An integer fits exactly when its nonzero bits fit the significand.
    // All 64-bit integers are in range for both floating-point formats.
    if !inexact || round == Round::NearestTiesToEven {
        let value = match (single, signed) {
            (true, true) => (bits as i64 as f32).to_bits() as u64,
            (true, false) => (bits as f32).to_bits() as u64,
            (false, true) => (bits as i64 as f64).to_bits(),
            (false, false) => (bits as f64).to_bits(),
        };
        return (if inexact { Status::INEXACT } else { Status::OK }).and(value);
    }
    match (single, signed) {
        (true, true) => Single::from_i128_r(bits as i64 as i128, round).map(|v| v.to_bits() as u64),
        (true, false) => Single::from_u128_r(bits as u128, round).map(|v| v.to_bits() as u64),
        (false, true) => Double::from_i128_r(bits as i64 as i128, round).map(|v| v.to_bits() as u64),
        (false, false) => Double::from_u128_r(bits as u128, round).map(|v| v.to_bits() as u64),
    }
}

#[inline]
fn convert_format(bits: u64, single: bool, round: Round) -> StatusAnd<u64> {
    if single {
        let value = f64::from_bits(bits);
        if value.is_finite() && round == Round::NearestTiesToEven {
            let rounded = value as f32;
            let inexact = rounded as f64 != value;
            let mut status = if inexact { Status::INEXACT } else { Status::OK };
            if rounded.is_infinite() {
                status |= Status::OVERFLOW;
            }
            if inexact && rounded.to_bits() & 0x7fffffff < 0x00800000 {
                status |= Status::UNDERFLOW;
            }
            return status.and(rounded.to_bits() as u64);
        }
        let converted: StatusAnd<Single> = Double::from_bits(bits.into()).convert_r(round, &mut false);
        converted.map(|v| v.to_bits() as u64)
    } else {
        let value = f32::from_bits(bits as u32);
        if value.is_finite() {
            return Status::OK.and((value as f64).to_bits());
        }
        let converted: StatusAnd<Double> = Single::from_bits(bits.into()).convert_r(round, &mut false);
        converted.map(|v| v.to_bits() as u64)
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

// f64 represents f32 products exactly and has enough precision to round f32
// sums and quotients. A discarded additive tail still sets INEXACT; division
// is exact precisely when the rounded quotient times the divisor is the input.
#[inline]
fn binary32(a_bits: u64, b_bits: u64, op: u32, round: Round) -> StatusAnd<u64> {
    if round == Round::NearestTiesToEven
        && a_bits & 0x7f800000 != 0x7f800000
        && b_bits & 0x7f800000 != 0x7f800000
        && (op != 12 || b_bits & 0x7fffffff != 0)
    {
        let a = f32::from_bits(a_bits as u32) as f64;
        let b = f32::from_bits(b_bits as u32) as f64;
        let value = match op {
            0 => a + b,
            4 => a - b,
            8 => a * b,
            12 => a / b,
            _ => unreachable!(),
        };
        let rounded = value as f32;
        let rhs = if op == 4 { -b } else { b };
        let inexact = if op == 12 {
            (rounded as f64) * b != a
        } else {
            rounded as f64 != value
                || ((op == 0 || op == 4) && ((value == a && rhs != 0.0) || (value == rhs && a != 0.0)))
        };
        let mut status = if inexact { Status::INEXACT } else { Status::OK };
        if rounded.is_infinite() {
            status |= Status::OVERFLOW;
        }
        if inexact && rounded.to_bits() & 0x7fffffff < 0x00800000 {
            status |= Status::UNDERFLOW;
        }
        return status.and(rounded.to_bits() as u64);
    }
    let a = Single::from_bits(a_bits.into());
    let b = Single::from_bits(b_bits.into());
    match op {
        0 => a.add_r(b, round),
        4 => a.sub_r(b, round),
        8 => a.mul_r(b, round),
        12 => a.div_r(b, round),
        _ => unreachable!(),
    }
    .map(|x| x.to_bits() as u64)
}

#[inline]
fn binary64(a_bits: u64, b_bits: u64, c_bits: u64, op: u32, rm: Round) -> StatusAnd<u64> {
    // These exponent bounds keep products and rounding residuals normal.
    let bounded = |x: u64| x & 0x7fffffffffffffff == 0 || (767..=1279).contains(&((x >> 52) & 2047));
    let sig = |x: u64| ((x & 0x000fffffffffffff) | (1 << 52)) as u128;
    let inexact = |x: u128| x != 0 && 128 - x.leading_zeros() - x.trailing_zeros() > 53;
    let software = || {
        let a = Double::from_bits(a_bits.into());
        let b = Double::from_bits(b_bits.into());
        let c = Double::from_bits(c_bits.into());
        match op {
            0 => a.add_r(b, rm),
            4 => a.sub_r(b, rm),
            8 => a.mul_r(b, rm),
            12 => a.div_r(b, rm),
            16 => a.mul_add_r(b, c, rm),
            _ => unreachable!(),
        }
        .map(|x| x.to_bits() as u64)
    };

    if rm != Round::NearestTiesToEven
        || !bounded(a_bits)
        || !bounded(b_bits)
        || (op == 16 && !bounded(c_bits))
        || (op == 12 && b_bits & 0x7fffffffffffffff == 0)
    {
        return software();
    }
    let a = f64::from_bits(a_bits);
    let b = f64::from_bits(b_bits);
    let c = f64::from_bits(c_bits);
    let value = match op {
        0 => a + b,
        4 => a - b,
        8 => a * b,
        12 => a / b,
        16 => a.mul_add(b, c),
        _ => unreachable!(),
    };
    let nx = match op {
        0 | 4 => {
            let rhs = if op == 4 { -b } else { b };
            let z = value - a;
            (a - (value - z)) + (rhs - z) != 0.0
        }
        8 => a != 0.0 && b != 0.0 && inexact(sig(a_bits) * sig(b_bits)),
        12 => value.mul_add(b, -a) != 0.0,
        16 => {
            if a == 0.0 || b == 0.0 {
                false
            } else if c == 0.0 {
                inexact(sig(a_bits) * sig(b_bits))
            } else {
                let exponent = |x: u64| ((x >> 52) & 2047) as i32 - 1023;
                let shift = exponent(c_bits) - exponent(a_bits) - exponent(b_bits) + 52;
                // Leave room for the sign and the carry of the exact i128 sum.
                if !(-20..=73).contains(&shift) {
                    return software();
                }
                let mut product = sig(a_bits) * sig(b_bits);
                let mut addend = sig(c_bits);
                if shift >= 0 {
                    addend <<= shift;
                } else {
                    product <<= -shift;
                }
                let product = if (a_bits ^ b_bits) >> 63 != 0 {
                    -(product as i128)
                } else {
                    product as i128
                };
                let addend = if c_bits >> 63 != 0 {
                    -(addend as i128)
                } else {
                    addend as i128
                };
                inexact((product + addend).unsigned_abs())
            }
        }
        _ => unreachable!(),
    };
    (if nx { Status::INEXACT } else { Status::OK }).and(value.to_bits())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_match_apfloat() {
        let rounds = [
            Round::NearestTiesToEven,
            Round::TowardZero,
            Round::TowardNegative,
            Round::TowardPositive,
            Round::NearestTiesToAway,
        ];
        let mut values = vec![
            0,
            1,
            u64::MAX,
            i64::MIN as u64,
            (1 << 24) - 1,
            (1 << 24) + 1,
            (1 << 53) - 1,
            (1 << 53) + 1,
            0x7fffffff,
            0x80000000,
            0xffffffff,
            0x7f800000,
            0x7f800001,
            0xffc01234,
            0x7ff0000000000000,
            0x7ff0000000000001,
            0xfff8000000001234,
        ];
        for value in [
            f32::MIN_POSITIVE as f64,
            f32::MAX as f64,
            f32::from_bits(1) as f64,
            0.5 * f32::from_bits(1) as f64,
        ] {
            for offset in [-1i64, 0, 1] {
                values.push(value.to_bits().wrapping_add_signed(offset));
                values.push(value.to_bits().wrapping_add_signed(offset) | (1 << 63));
            }
        }
        let mut seed = 0xd479c131a0f537e1u64;
        for _ in 0..100_000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            values.push(seed);
        }
        for bits in values {
            for round in rounds {
                for single in [true, false] {
                    let source = if single { bits } else { bits as u32 as u64 };
                    let expected = if single {
                        let result: StatusAnd<Single> = Double::from_bits(source.into()).convert_r(round, &mut false);
                        result.map(|v| v.to_bits() as u64)
                    } else {
                        let result: StatusAnd<Double> = Single::from_bits(source.into()).convert_r(round, &mut false);
                        result.map(|v| v.to_bits() as u64)
                    };
                    assert_eq!(
                        convert_format(source, single, round),
                        expected,
                        "format bits={source:x} single={single} round={round:?}"
                    );
                    for kind in 0..4 {
                        let signed = if kind == 0 {
                            bits as i32 as i128
                        } else {
                            bits as i64 as i128
                        };
                        let unsigned = if kind == 1 { bits as u32 as u128 } else { bits as u128 };
                        let expected = match (single, kind & 1 == 0) {
                            (true, true) => Single::from_i128_r(signed, round).map(|v| v.to_bits() as u64),
                            (true, false) => Single::from_u128_r(unsigned, round).map(|v| v.to_bits() as u64),
                            (false, true) => Double::from_i128_r(signed, round).map(|v| v.to_bits() as u64),
                            (false, false) => Double::from_u128_r(unsigned, round).map(|v| v.to_bits() as u64),
                        };
                        assert_eq!(
                            from_integer(bits, kind, single, round),
                            expected,
                            "integer bits={bits:x} kind={kind} single={single} round={round:?}"
                        );
                    }
                }
            }
        }
    }
    fn reference(a: u64, b: u64, op: u32, round: Round) -> StatusAnd<u64> {
        let a = Single::from_bits(a.into());
        let b = Single::from_bits(b.into());
        match op {
            0 => a.add_r(b, round),
            4 => a.sub_r(b, round),
            8 => a.mul_r(b, round),
            12 => a.div_r(b, round),
            _ => unreachable!(),
        }
        .map(|x| x.to_bits() as u64)
    }
    #[test]
    fn binary32_matches_apfloat() {
        let edges = [
            0, 1, 2, 0x003fffff, 0x00400000, 0x007fffff, 0x00800000, 0x00800001, 0x3f000000, 0x3f7fffff, 0x3f800000,
            0x3f800001, 0x40000000, 0x7f000000, 0x7f7fffff, 0x7f800000, 0x7f800001, 0x7fc00000,
        ];
        for round in [
            Round::NearestTiesToEven,
            Round::TowardZero,
            Round::TowardNegative,
            Round::TowardPositive,
            Round::NearestTiesToAway,
        ] {
            for x in edges {
                for y in edges {
                    for sign_a in [0, 0x80000000] {
                        for sign_b in [0, 0x80000000] {
                            for op in [0, 4, 8, 12] {
                                check(x | sign_a, y | sign_b, op, round);
                            }
                        }
                    }
                }
            }
        }
        let mut seed = 0x684126735a98b401u64;
        for _ in 0..1_000_000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let a = seed as u32 as u64;
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let b = seed as u32 as u64;
            for op in [0, 4, 8, 12] {
                check(a, b, op, Round::NearestTiesToEven);
            }
        }
    }
    fn check(a: u64, b: u64, op: u32, round: Round) {
        let got = binary32(a, b, op, round);
        let expected = reference(a, b, op, round);
        assert_eq!(
            (got.status, got.value),
            (expected.status, expected.value),
            "a={a:08x} b={b:08x} op={op} rm={round:?}"
        );
    }
    fn reference64(a: u64, b: u64, c: u64, op: u32, rm: Round) -> StatusAnd<u64> {
        let a = Double::from_bits(a.into());
        let b = Double::from_bits(b.into());
        let c = Double::from_bits(c.into());
        match op {
            0 => a.add_r(b, rm),
            4 => a.sub_r(b, rm),
            8 => a.mul_r(b, rm),
            12 => a.div_r(b, rm),
            16 => a.mul_add_r(b, c, rm),
            _ => unreachable!(),
        }
        .map(|x| x.to_bits() as u64)
    }
    #[test]
    fn binary64_matches_apfloat() {
        let edges = [
            0,
            1,
            0x000fffffffffffff,
            0x0010000000000000,
            767u64 << 52,
            (1279u64 << 52) | 0xfffffffffffff,
            0x3ff0000000000000,
            0x3fefffffffffffff,
            0x3ff0000000000001,
            0x7fefffffffffffff,
            0x7ff0000000000000,
            0x7ff0000000000001,
            0x7ff8000000000000,
        ];
        for rm in [
            Round::NearestTiesToEven,
            Round::TowardZero,
            Round::TowardNegative,
            Round::TowardPositive,
            Round::NearestTiesToAway,
        ] {
            for a in edges {
                for b in edges {
                    for c in edges {
                        for sign in 0..8 {
                            for op in [0, 4, 8, 12, 16] {
                                check64(
                                    a | ((sign & 1) << 63),
                                    b | (((sign >> 1) & 1) << 63),
                                    c | (((sign >> 2) & 1) << 63),
                                    op,
                                    rm,
                                );
                            }
                        }
                    }
                }
            }
        }
        let mut seed = 0x42681e735a98b401u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for mode in 0..3 {
            for _ in 0..1_000_000 {
                let fix = |x: u64| match mode {
                    0 => x,
                    1 => (x & 0x800fffffffffffff) | (((x >> 52) % 513 + 767) << 52),
                    _ => (x & 0x800fffffffffffff) | (((x >> 52) % 65 + 991) << 52),
                };
                let a = fix(next());
                let b = fix(next());
                let c = fix(next());
                for op in [0, 4, 8, 12, 16] {
                    check64(a, b, c, op, Round::NearestTiesToEven);
                }
            }
        }
    }
    fn check64(a: u64, b: u64, c: u64, op: u32, rm: Round) {
        let x = binary64(a, b, c, op, rm);
        let y = reference64(a, b, c, op, rm);
        assert_eq!(
            (x.status, x.value),
            (y.status, y.value),
            "a={a:016x} b={b:016x} c={c:016x} op={op} rm={rm:?}"
        );
    }
}
