use generic::timing::{CacheTiming, CpuTiming};

/// Chipyard's Rocket with its L1 caches: an instruction a cycle from the instruction cache, the
/// target of a taken branch or jump a cycle later; its data cache answers a hit without stalling
/// the pipeline, and its multiplier is pipelined: a dependent use waits 3 cycles.
pub struct CachedRocket {
    next_pc: u64,
    /// The line of the last fetch: it is in the instruction cache, a fetch from it asks nothing.
    line: u64,
    icache: Box<dyn CacheTiming>,
}

impl CachedRocket {
    pub fn new(icache: Box<dyn CacheTiming>) -> Self {
        Self {
            next_pc: 0,
            line: u64::MAX,
            icache,
        }
    }
}

impl CpuTiming for CachedRocket {
    fn instruction(&mut self, pc: u64, instruction: u32, _decoded: u32, _operand: u64) -> u64 {
        let compressed = instruction & 3 != 3;
        let mut cycles = 1 + u64::from(pc != self.next_pc);
        if pc >> 6 != self.line {
            cycles += self.icache.access(pc);
            self.line = pc >> 6;
        }
        self.next_pc = pc.wrapping_add(if compressed { 2 } else { 4 });
        let opcode = instruction & 127;
        if !compressed && matches!(opcode, 0x33 | 0x3b) && instruction >> 25 == 1 {
            cycles += if (instruction >> 12) & 7 < 4 {
                3
            } else if opcode == 0x3b {
                32
            } else {
                64
            };
        }
        cycles
    }
}
