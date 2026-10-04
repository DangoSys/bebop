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

pub(crate) fn arithmetic(a: u64, b: u64, c: u64, operation: u32, bits: usize, rm: Round) -> StatusAnd<u64> {
    match bits {
        32 => arithmetic_format::<Single, WideSingle>(a, b, c, operation, rm),
        64 => arithmetic_format::<Double, WideDouble>(a, b, c, operation, rm),
        _ => unreachable!(),
    }
}

pub(crate) fn convert(a: u64, source_bits: usize, rm: Round) -> StatusAnd<u64> {
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

    #[test]
    fn fused_result_rounded_to_normal_can_underflow() {
        for (bits, a, b, c, expected) in [
            (32, 1, 0x3f000000, 0x00800000, 0x80800000),
            (64, 1, 0x3fe0000000000000, 0x0010000000000000, 0x8010000000000000),
        ] {
            for rounding in [
                Round::NearestTiesToEven,
                Round::TowardZero,
                Round::TowardNegative,
                Round::TowardPositive,
                Round::NearestTiesToAway,
            ] {
                let result = arithmetic(a, b, c, 0x47, bits, rounding);
                let magnitude_down = matches!(rounding, Round::TowardZero | Round::TowardPositive);
                assert_eq!(result.value, expected - u64::from(magnitude_down));
                assert_eq!(result.status, Status::UNDERFLOW | Status::INEXACT);
            }
            let normal = arithmetic(a, b, c, 0x43, bits, Round::NearestTiesToEven);
            assert_eq!(normal.value, c);
            assert_eq!(normal.status, Status::INEXACT);
        }
    }

    #[test]
    fn narrowing_tininess_uses_unbounded_exponent() {
        let tiny = convert(0x380fffffe1000000, 64, Round::NearestTiesToEven);
        assert_eq!(tiny.value, 0x00800000);
        assert_eq!(tiny.status, Status::UNDERFLOW | Status::INEXACT);
        let normal = convert(0x380ffffff0000000, 64, Round::NearestTiesToEven);
        assert_eq!(normal.value, 0x00800000);
        assert_eq!(normal.status, Status::INEXACT);
        let exact = convert(0x3810000000000000, 64, Round::NearestTiesToEven);
        assert_eq!(exact.value, 0x00800000);
        assert_eq!(exact.status, Status::OK);
    }
    #[test]
    fn directed_overflow_to_finite_raises_overflow() {
        for (bits, maximum, two, sign) in [
            (32, 0x7f7fffff, 0x40000000, 0x80000000),
            (64, 0x7fefffffffffffff, 0x4000000000000000, 0x8000000000000000),
        ] {
            for (negative, rounding) in [
                (false, Round::TowardZero),
                (false, Round::TowardNegative),
                (true, Round::TowardZero),
                (true, Round::TowardPositive),
            ] {
                let a = maximum | if negative { sign } else { 0 };
                let result = arithmetic(a, two, 0, 8, bits, rounding);
                assert_eq!(result.value, a);
                assert_eq!(result.status, Status::OVERFLOW | Status::INEXACT);
            }
            let below_overflow = arithmetic(maximum, 1, 0, 0, bits, Round::TowardZero);
            assert_eq!(below_overflow.value, maximum);
            assert_eq!(below_overflow.status, Status::INEXACT);
        }
    }

    #[test]
    fn narrowing_finite_overflow_and_inexact_boundary() {
        let overflow = convert(0x47f0000000000000, 64, Round::TowardZero);
        assert_eq!(overflow.value, 0x7f7fffff);
        assert_eq!(overflow.status, Status::OVERFLOW | Status::INEXACT);
        let inexact = convert(0x47efffffe0000001, 64, Round::TowardZero);
        assert_eq!(inexact.value, 0x7f7fffff);
        assert_eq!(inexact.status, Status::INEXACT);
        let exact = convert(0x47efffffe0000000, 64, Round::TowardZero);
        assert_eq!(exact.value, 0x7f7fffff);
        assert_eq!(exact.status, Status::OK);
    }
}
