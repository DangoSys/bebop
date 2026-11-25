use rustc_apfloat::{
    ieee::{Double, IeeeFloat, Semantics, Single},
    Float, FloatConvert, Round, Status, StatusAnd,
};

struct WideSingleSemantics;
impl Semantics for WideSingleSemantics {
    const BITS: usize = 39;
    const EXP_BITS: usize = 15;
}
struct WideDoubleSemantics;
impl Semantics for WideDoubleSemantics {
    const BITS: usize = 68;
    const EXP_BITS: usize = 15;
}
type WideSingle = IeeeFloat<WideSingleSemantics>;
type WideDouble = IeeeFloat<WideDoubleSemantics>;

fn calculate<T: Float>(a: T, b: T, c: T, operation: u32, rm: Round) -> StatusAnd<T> {
    match operation {
        0 => a.add_r(b, rm),
        4 => a.sub_r(b, rm),
        8 => a.mul_r(b, rm),
        12 => a.div_r(b, rm),
        0x43 => a.mul_add_r(b, c, rm),
        0x47 => a.mul_add_r(b, -c, rm),
        0x4b => (-a).mul_add_r(b, c, rm),
        0x4f => (-a).mul_add_r(b, -c, rm),
        _ => unreachable!(),
    }
}

fn arithmetic_format<T, W>(a: u64, b: u64, c: u64, operation: u32, rm: Round) -> StatusAnd<u64>
where
    T: Float + FloatConvert<W>,
    W: Float,
{
    let a = T::from_bits(a.into());
    let b = T::from_bits(b.into());
    let c = T::from_bits(c.into());
    let mut result = calculate(a, b, c, operation, rm);
    // IEEE range exceptions use destination precision with an unbounded exponent.
    // APFloat misses tininess rounded to minnormal and directed finite overflow.
    if result.status.contains(Status::INEXACT)
        && (result.value.abs() == T::smallest_normalized() || result.value.abs() == T::largest())
    {
        let wide = |value: T| value.convert_r(Round::NearestTiesToEven, &mut false).value;
        let rounded = calculate(wide(a), wide(b), wide(c), operation, rm).value.abs();
        if rounded < wide(T::smallest_normalized()) {
            result.status |= Status::UNDERFLOW;
        }
        if rounded > wide(T::largest()) {
            result.status |= Status::OVERFLOW;
        }
    }
    result.map(|value| value.to_bits() as u64)
}

/// Single precision rounded to nearest even, on the host's arithmetic: the host rounds as
/// IEEE 754 does, and whether its result is exact is the inexact flag. None where a special
/// value or another flag can come up (a non-finite operand, a result that is not a normal
/// number strictly inside the format's range, an inexact zero): the soft float decides those.
fn single(a: u64, b: u64, c: u64, operation: u32) -> Option<StatusAnd<u64>> {
    let a = f32::from_bits(a as u32);
    let b = f32::from_bits(b as u32);
    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    let (value, exact) = match operation {
        0 | 4 => {
            let b = if operation == 4 { -b } else { b };
            let sum = a + b;
            // The rounding error of a sum is itself a float (two-sum).
            let rest = sum - a;
            (sum, (a - (sum - rest)) + (b - rest) == 0.0)
        }
        // The product of two singles is exact in a double.
        8 => {
            let product = a as f64 * b as f64;
            (product as f32, (product as f32) as f64 == product)
        }
        12 => {
            if b == 0.0 {
                return None;
            }
            let quotient = a / b;
            (quotient, quotient as f64 * b as f64 == a as f64)
        }
        0x43 | 0x47 | 0x4b | 0x4f => {
            let c = f32::from_bits(c as u32);
            if !c.is_finite() {
                return None;
            }
            let a = if operation & 8 != 0 { -a } else { a };
            let c = if operation & 4 != 0 { -c } else { c };
            let value = a.mul_add(b, c);
            let product = a as f64 * b as f64;
            let sum = product + c as f64;
            let rest = sum - product;
            let exact = (product - (sum - rest)) + (c as f64 - rest) == 0.0 && value as f64 == sum;
            (value, exact)
        }
        _ => unreachable!(),
    };
    let inside = value.is_normal() && value.abs() > f32::MIN_POSITIVE && value.abs() < f32::MAX;
    if !inside && !(value == 0.0 && exact) {
        return None;
    }
    let status = if exact { Status::OK } else { Status::INEXACT };
    Some(status.and(value.to_bits() as u64))
}

pub fn arithmetic(a: u64, b: u64, c: u64, operation: u32, bits: usize, rm: Round) -> StatusAnd<u64> {
    if bits == 32 && rm == Round::NearestTiesToEven {
        if let Some(result) = single(a, b, c, operation) {
            return result;
        }
    }
    match bits {
        32 => arithmetic_format::<Single, WideSingle>(a, b, c, operation, rm),
        64 => arithmetic_format::<Double, WideDouble>(a, b, c, operation, rm),
        _ => unreachable!(),
    }
}

pub fn convert(a: u64, source_bits: usize, rm: Round) -> StatusAnd<u64> {
    match source_bits {
        32 => {
            let result: StatusAnd<Double> = Single::from_bits(a.into()).convert_r(rm, &mut false);
            result.map(|value| value.to_bits() as u64)
        }
        64 => {
            let input = Double::from_bits(a.into());
            let mut result: StatusAnd<Single> = input.convert_r(rm, &mut false);
            if result.status.contains(Status::INEXACT)
                && (result.value.abs() == Single::smallest_normalized() || result.value.abs() == Single::largest())
            {
                let rounded: StatusAnd<WideSingle> = input.convert_r(rm, &mut false);
                let minimum: StatusAnd<WideSingle> =
                    Single::smallest_normalized().convert_r(Round::NearestTiesToEven, &mut false);
                let maximum: StatusAnd<WideSingle> = Single::largest().convert_r(Round::NearestTiesToEven, &mut false);
                if rounded.value.abs() < minimum.value {
                    result.status |= Status::UNDERFLOW;
                }
                if rounded.value.abs() > maximum.value {
                    result.status |= Status::OVERFLOW;
                }
            }
            result.map(|value| value.to_bits() as u64)
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The host path agrees with the soft float in value and flags wherever it answers.
    #[test]
    fn single_on_host_matches_soft_float() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        // Operands near each other, far apart, tiny, huge, and exact small integers.
        let mut operand = move || {
            let bits = next();
            match bits >> 61 {
                0 => (bits as u32 % 4096) as f32,
                1 => f32::from_bits((bits as u32 & 0x807f_ffff) | 0x3f80_0000),
                2 => f32::from_bits((bits as u32 & 0x80ff_ffff) | 0x0080_0000),
                3 => f32::from_bits((bits as u32 & 0x80ff_ffff) | 0x7e80_0000),
                4 => f32::from_bits(bits as u32 & 0x8000_0fff),
                _ => f32::from_bits(bits as u32),
            }
            .to_bits() as u64
        };
        let mut answered = 0;
        for _ in 0..2_000_000 {
            let (a, b, c) = (operand(), operand(), operand());
            for operation in [0, 4, 8, 12, 0x43, 0x47, 0x4b, 0x4f] {
                let Some(host) = single(a, b, c, operation) else {
                    continue;
                };
                let soft = arithmetic_format::<Single, WideSingle>(a, b, c, operation, Round::NearestTiesToEven);
                assert_eq!(
                    (host.value, host.status),
                    (soft.value, soft.status),
                    "operation {operation:#x} on {a:#x} {b:#x} {c:#x}"
                );
                answered += 1;
            }
        }
        assert!(answered > 8_000_000);
    }
}
