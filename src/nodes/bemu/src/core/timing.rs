use crate::configs::types::CpuPipeline;

pub(super) struct Timing {
    pipeline: CpuPipeline,
    next_pc: u64,
}

impl Timing {
    pub(super) fn new(pipeline: CpuPipeline) -> Self {
        Self {
            pipeline,
            next_pc: 0,
        }
    }

    pub(super) fn instruction(&mut self, pc: u64, instruction: u32) -> u64 {
        let compressed = instruction & 3 != 3;
        let length = if compressed { 2 } else { 4 };
        let mut cycles = match self.pipeline {
            CpuPipeline::Sequential => {
                if compressed {
                    14
                } else {
                    26
                }
            }
            CpuPipeline::Rocket => {
                // Only a sequential instruction at the upper halfword reuses a fetch packet.
                1 + 12 * u64::from(pc != self.next_pc || pc & 3 == 0)
                    + 12 * u64::from(!compressed && pc & 3 == 2)
            }
        };
        self.next_pc = pc.wrapping_add(length);
        let opcode = instruction & 127;
        let memory = if compressed {
            matches!(instruction & 3, 0 | 2) && matches!(instruction >> 13, 1 | 2 | 3 | 5 | 6 | 7)
        } else {
            matches!(opcode, 3 | 7 | 35 | 39 | 47)
        };
        // PhysicalAccess and the coherent cache request/response path.
        if memory {
            cycles += 25;
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

#[cfg(test)]
mod tests {
    use super::{CpuPipeline, Timing};

    #[test]
    fn rocket_reuses_sequential_packets_and_refetches_after_a_redirect() {
        let mut timing = Timing::new(CpuPipeline::Rocket);
        for (pc, instruction, cycles) in [
            (0x1000, 0x1, 13),
            (0x1002, 0x1, 1),
            (0x1004, 0x13, 13),
            (0x1008, 0x1, 13),
            (0x100a, 0x13, 13),
            (0x100e, 0x1, 1),
            (0x1012, 0x1, 13),
        ] {
            assert_eq!(timing.instruction(pc, instruction), cycles);
        }
    }
}
