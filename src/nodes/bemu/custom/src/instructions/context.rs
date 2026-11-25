pub use generic::blink::ExecContext;

/// Instruction trait - all instructions must implement this
pub trait Instruction {
    /// Instruction opcode (funct7 field)
    const FUNCT: u32;

    /// Execute the instruction, return result value
    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64;
    /// Local timing contract; callers must model overlap and contention separately.
    fn latency(xs1: u64, xs2: u64, _ctx: &ExecContext) -> u64;
}
