use generic::timing::CpuTiming;

/// The base CPU, without instruction-level or memory-level parallelism: one instruction at a time,
/// each fetched, read, executed and retired before the next, its memory access through the hart's
/// memory system. With `fetch_line` the sequencer holds the line of its last fetch.
pub struct Sequential {
    fetch_line: bool,
    /// The line the sequencer holds from its last fetch.
    line: Option<u64>,
}

impl Sequential {
    pub fn new(fetch_line: bool) -> Self {
        Self { fetch_line, line: None }
    }
}

impl CpuTiming for Sequential {
    fn instruction(&mut self, pc: u64, instruction: u32, _decoded: u32, _operand: u64) -> u64 {
        let compressed = instruction & 3 != 3;
        let mut cycles = if self.fetch_line {
            // The low halfword: a cycle from the held line, or a fetch of its line, which
            // the sequencer then holds. The high halfword of a 4-byte instruction comes
            // with a held low one, takes a cycle after a fetched low one, or is a fetch of
            // the next line. Operands and execute follow.
            let fetched = self.line != Some(pc >> 6);
            self.line = Some(pc >> 6);
            let mut cycles = 2 + if fetched { 4 } else { 1 };
            if !compressed {
                if self.line != Some((pc + 2) >> 6) {
                    self.line = Some((pc + 2) >> 6);
                    cycles += 4;
                } else if fetched {
                    cycles += 1;
                }
            }
            cycles
        } else if compressed {
            6
        } else {
            10
        };
        let opcode = instruction & 127;
        // fence, fence.i, sfence.vma and a CSR write drop the held line.
        if !compressed
            && (opcode == 0x0f || (opcode == 0x73 && ((instruction >> 12) & 7 != 0 || instruction >> 25 == 9)))
        {
            self.line = None;
        }
        let memory = if compressed {
            matches!(instruction & 3, 0 | 2) && matches!(instruction >> 13, 1 | 2 | 3 | 5 | 6 | 7)
        } else {
            matches!(opcode, 3 | 7 | 35 | 39 | 47)
        };
        // The sequencer's access reaches the cache when its authorization answers, and the cache's
        // result returns in the cycle it is ready.
        if memory {
            cycles += 4;
        }
        if !compressed && matches!(opcode, 0x33 | 0x3b) && instruction >> 25 == 1 {
            cycles += if (instruction >> 12) & 7 < 4 {
                8
            } else if opcode == 0x3b {
                32
            } else {
                64
            };
        }
        cycles
    }
}
