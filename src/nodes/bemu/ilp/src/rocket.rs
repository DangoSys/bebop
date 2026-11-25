use generic::timing::{CacheTiming, CpuTiming};

/// Where a result becomes usable.
#[derive(Clone, Copy)]
enum Result {
    None,
    Integer,
    Float,
}

/// What the pipeline needs of an instruction, worked out once from its 4-byte form.
#[derive(Clone, Copy)]
struct Shape {
    insn: u32,
    /// The integer registers it waits for (x0 is always ready) and the floating-point ones (32 is none).
    x: [u8; 2],
    f: [u8; 3],
    rd: u8,
    /// The offset of its data access from rs1.
    memory: Option<i32>,
    muldiv: bool,
    divide_f: bool,
    result: Result,
    /// The cycles from its retirement to its result.
    latency: u8,
    /// fence.i and sfence.vma drop the I$.
    invalidates: bool,
}

impl Shape {
    fn new(raw: u32) -> Self {
        let opcode = raw & 127;
        let funct3 = (raw >> 12) & 7;
        let funct7 = raw >> 25;
        let rd = ((raw >> 7) & 31) as u8;
        let rs1 = ((raw >> 15) & 31) as u8;
        let rs2 = ((raw >> 20) & 31) as u8;
        let rs3 = (raw >> 27) as u8;
        // Floating-point operations whose rs1 is an integer register, and whose rd is one.
        let integer_source = opcode == 0x53 && matches!(funct7, 0x68 | 0x69 | 0x78 | 0x79);
        let integer_result = opcode == 0x53 && matches!(funct7, 0x50 | 0x51 | 0x60 | 0x61 | 0x70 | 0x71);
        let floating = matches!(opcode, 0x53 | 0x43 | 0x47 | 0x4b | 0x4f);
        let reads_rs1 =
            !matches!(opcode, 0x37 | 0x17 | 0x6f) && !(opcode == 0x73 && funct3 >= 4) && (!floating || integer_source);
        let reads_rs2 = matches!(opcode, 0x23 | 0x2f | 0x33 | 0x3b | 0x63 | 0x0b | 0x7b);
        let float_sources = floating && !integer_source;
        let divide_f = opcode == 0x53 && matches!(funct7 >> 2, 0x03 | 0x0b);
        let muldiv = matches!(opcode, 0x33 | 0x3b) && funct7 == 1;
        let (result, latency) = if floating && !integer_result || opcode == 0x07 {
            let latency = if opcode == 0x07 {
                2
            } else if divide_f {
                58
            } else if opcode != 0x53 || funct7 >> 2 <= 0x02 {
                8
            } else {
                4
            };
            (Result::Float, latency)
        } else {
            let latency = if muldiv {
                match (funct3 < 4, opcode == 0x3b) {
                    (true, false) => 66,
                    (true, true) => 34,
                    (false, false) => 67,
                    (false, true) => 35,
                }
            } else if integer_result {
                4
            } else if opcode == 0x03 {
                if funct3 & 3 < 2 {
                    3
                } else {
                    2
                }
            } else {
                1
            };
            if rd != 0 && !matches!(opcode, 0x23 | 0x27 | 0x63) {
                (Result::Integer, latency)
            } else {
                (Result::None, latency)
            }
        };
        Self {
            insn: raw,
            x: [if reads_rs1 { rs1 } else { 0 }, if reads_rs2 { rs2 } else { 0 }],
            f: [
                if float_sources { rs1 } else { 32 },
                if float_sources || opcode == 0x27 { rs2 } else { 32 },
                if matches!(opcode, 0x43 | 0x47 | 0x4b | 0x4f) {
                    rs3
                } else {
                    32
                },
            ],
            rd,
            memory: match opcode {
                0x03 | 0x07 => Some((raw as i32) >> 20),
                0x23 | 0x27 => Some((((raw as i32) >> 25) << 5) | ((raw >> 7) & 31) as i32),
                0x2f => Some(0),
                _ => None,
            },
            muldiv,
            divide_f,
            result,
            latency,
            invalidates: opcode == 0x0f && funct3 == 1 || opcode == 0x73 && funct7 == 9,
        }
    }
}

/// Rocket's in-order pipeline: an instruction retires a cycle after the previous one unless it
/// waits on a redirect, an operand, a busy unit or its caches.
pub struct Rocket {
    next_pc: u64,
    /// The line of the last fetch: it is in the instruction cache, a fetch from it asks nothing.
    line: u64,
    /// The cycle the last instruction retired, the cycle each integer register's value can be
    /// used by a retiring instruction (each floating-point one's, with one that is always ready),
    /// and the cycle the multiplier/divider is free.
    clock: u64,
    ready: [u64; 32],
    ready_f: [u64; 33],
    muldiv_free: u64,
    divide_f_free: u64,
    /// The shapes of the instructions seen, by a hash of their bits.
    shapes: Box<[Shape; 1024]>,
    icache: Box<dyn CacheTiming>,
    dcache: Box<dyn CacheTiming>,
}

impl Rocket {
    pub fn new(icache: Box<dyn CacheTiming>, dcache: Box<dyn CacheTiming>) -> Self {
        Self {
            next_pc: 0,
            line: u64::MAX,
            clock: 0,
            ready: [0; 32],
            ready_f: [0; 33],
            muldiv_free: 0,
            divide_f_free: 0,
            shapes: Box::new([Shape::new(0); 1024]),
            icache,
            dcache,
        }
    }
}

impl CpuTiming for Rocket {
    /// Rocket with its L1 I$ and D$: an instruction retires a cycle after the previous one, a
    /// redirect costs 3 more (one more when its target is a 4-byte instruction in the upper
    /// halfword), and an I$ miss refills its line. A load's value is usable 2 cycles after it
    /// retires (3 for a byte or halfword); a multiply or divide occupies the unit and its result
    /// comes 34 to 67 cycles after it retires. A D$ hit costs nothing more and a miss refills its
    /// line (by virtual address). An FMA-class result is usable 8 cycles after it retires, a
    /// floating-point divide or square root occupies its unit for 58, other FP results 4.
    /// `operand` is the register rs1 names, before the instruction.
    fn instruction(&mut self, pc: u64, instruction: u32, raw: u32, operand: u64) -> u64 {
        let compressed = instruction & 3 != 3;
        let slot = &mut self.shapes[(raw ^ (raw >> 10) ^ (raw >> 20)) as usize & 1023];
        if slot.insn != raw {
            *slot = Shape::new(raw);
        }
        let shape = *slot;
        let mut start = self.clock + 1;
        if pc != self.next_pc {
            start += 3 + u64::from(!compressed && pc & 3 == 2);
        }
        if pc >> 6 != self.line {
            start += self.icache.access(pc);
            self.line = pc >> 6;
        }
        start = start
            .max(self.ready[shape.x[0] as usize])
            .max(self.ready[shape.x[1] as usize])
            .max(self.ready_f[shape.f[0] as usize])
            .max(self.ready_f[shape.f[1] as usize])
            .max(self.ready_f[shape.f[2] as usize]);
        if shape.divide_f {
            start = start.max(self.divide_f_free);
        }
        if let Some(offset) = shape.memory {
            start += self.dcache.access(operand.wrapping_add(offset as i64 as u64));
        }
        if shape.muldiv {
            start = start.max(self.muldiv_free);
            self.muldiv_free = start + u64::from(shape.latency);
        }
        if shape.divide_f {
            self.divide_f_free = start + u64::from(shape.latency);
        }
        match shape.result {
            Result::Float => self.ready_f[shape.rd as usize] = start + u64::from(shape.latency),
            Result::Integer => self.ready[shape.rd as usize] = start + u64::from(shape.latency),
            Result::None => {}
        }
        if shape.invalidates {
            self.icache.invalidate();
            self.line = u64::MAX;
        }
        let cycles = start - self.clock;
        self.clock = start;
        self.next_pc = pc.wrapping_add(if compressed { 2 } else { 4 });
        cycles
    }
}
