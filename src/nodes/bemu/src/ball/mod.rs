pub use crate::thread::instructions::context::ExecContext;
pub use linkme;
use linkme::distributed_slice;

pub type BallStates = std::collections::BTreeMap<(u32, std::any::TypeId), Box<dyn std::any::Any + Send>>;

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
    pub cycles: fn(&str, u32, u64, u64) -> Option<u64>,
}

/// Every Ball model linked into the simulation registers itself here; the
/// simulation entry links the Balls its design.pb names.
#[distributed_slice]
pub static BALLS: [Ball];

pub(crate) fn execute_known(ball_class: &str, funct: u32, xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64 {
    BALLS
        .iter()
        .find_map(|ball| (ball.execute)(ball_class, funct, xs1, xs2, ctx))
        .unwrap_or_else(|| panic!("no BEMU ball implementation for ballClass={ball_class} funct7={funct}"))
}

pub(crate) fn cycles_after_issue(ball_class: &str, funct: u32, xs1: u64, xs2: u64) -> u64 {
    BALLS
        .iter()
        .find_map(|ball| (ball.cycles)(ball_class, funct, xs1, xs2))
        .unwrap_or_else(|| panic!("no BEMU latency contract for ballClass={ball_class} funct7={funct}"))
}
