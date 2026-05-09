//===- 00_fence.rs - FENCE instruction -------------------------------------===//

use super::instruction::{ExecContext, Instruction};

pub struct Fence;

impl Instruction for Fence {
    const FUNCT: u32 = 0;

    fn exec(_xs1: u64, _xs2: u64, _ctx: &mut ExecContext) -> u64 {
        0
    }

    fn latency(_xs1: u64, _xs2: u64) -> u64 {
        1
    }
}
