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
