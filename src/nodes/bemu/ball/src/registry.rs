use generic::blink::{BallBus, ExecContext};
use linkme::distributed_slice;

/// Ball semantics are selected by the ThreadModel's ballISA mnemonic mapping.
/// Ball implementations must not own a numeric funct7 encoding.
pub trait BallModel {
    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64;
    fn latency(xs1: u64, xs2: u64) -> u64;
}

/// The model of one Ball: it answers instructions of its own Ball class and
/// declines every other class.
pub struct Ball {
    pub execute: fn(&str, u32, u64, u64, &mut ExecContext) -> Option<u64>,
    pub cycles: fn(&str, u32, u64, u64, &ExecContext) -> Option<u64>,
}

/// Every Ball model linked into the simulation registers itself here; the
/// simulation entry links the Balls its design.pb names.
#[distributed_slice]
pub static BALLS: [Ball];

fn execute_known(ball_class: &str, funct: u32, xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
    BALLS
        .iter()
        .find_map(|ball| (ball.execute)(ball_class, funct, xs1, xs2, ctx))
        .unwrap_or_else(|| panic!("no BEMU ball implementation for ballClass={ball_class} funct7={funct}"))
}

fn cycles_after_issue(ball_class: &str, funct: u32, xs1: u64, xs2: u64, ctx: &ExecContext) -> u64 {
    BALLS
        .iter()
        .find_map(|ball| (ball.cycles)(ball_class, funct, xs1, xs2, ctx))
        .unwrap_or_else(|| panic!("no BEMU latency contract for ballClass={ball_class} funct7={funct}"))
}

/// The Ball bus (arch: BBus): an instruction goes to the Ball of its class.
pub const BUS: BallBus = BallBus {
    execute: execute_known,
    cycles: cycles_after_issue,
};
