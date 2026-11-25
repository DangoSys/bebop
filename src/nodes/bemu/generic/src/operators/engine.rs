use super::operator_memory::OperatorMemory;
use super::vector::Vector;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fault {
    pub pc: u32,
    pub instruction: u32,
    pub cause: u32,
    pub value: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryError;

pub trait Memory {
    fn read(&mut self, address: u64, bytes: usize) -> Result<u64, MemoryError>;
    fn write(&mut self, address: u64, bytes: usize, value: u64) -> Result<(), MemoryError>;
    fn ball_command(&mut self, funct7: u32, rs1: u64, rs2: u64, elapsed: u64) -> Result<(u64, u64), MemoryError>;
}

struct InstructionProfile {
    path: std::path::PathBuf,
    counts: std::collections::BTreeMap<(u32, u32, u32), (u64, u64)>,
}

pub struct Engine {
    pub x: [u64; 32],
    pub f: [u64; 32],
    pub fcsr: u8,
    pub vector: Vector,
    pub programs: [Vec<u8>; 2],
    pub loaded: [usize; 2],
    pub constants: [Vec<u8>; 2],
    pub pc: u32,
    pub entry: u32,
    pub instruction: u32,
    pub cycles: u64,
    /// The cycle the VRF writes of the last commit end (arch: Operands writes behind the next instruction).
    pub vrf_ready: u64,
    pub lanes: usize,
    pub base_integer: bool,
    pub fp64: bool,
    pub memory_ports: usize,
    pub bank_bytes: usize,
    pub operand_cache: Vec<Option<usize>>,
    pub operand_cache_next: usize,
    /// The states of the operand cache the timing memo has met, by id, and the id of each (timing.rs).
    pub operand_states: Vec<Vec<Option<usize>>>,
    pub operand_state_ids:
        std::collections::HashMap<Vec<Option<usize>>, u32, std::hash::BuildHasherDefault<super::timing::KeyHasher>>,
    /// The id of the cache's state, where known; `operand_cache` holds that state unless it is stale.
    pub operand_state: Option<u32>,
    pub operand_stale: bool,
    /// Cycles of vector instructions already timed with the same inputs (timing.rs).
    pub timing_memo: std::collections::HashMap<
        super::timing::TimingKey,
        super::timing::TimingResult,
        std::hash::BuildHasherDefault<super::timing::KeyHasher>,
    >,
    profile: Option<InstructionProfile>,
}

impl Engine {
    pub fn new(
        config: &crate::configs::GpDomainConfig,
        bank_bytes: usize,
        profile_path: Option<std::path::PathBuf>,
    ) -> Self {
        let vlen_bits = config.v_len as usize;
        let elen_bits = config.e_len as usize;
        let instruction_bytes = config.i_buf_words as usize * 4;
        assert!(vlen_bits >= 128 && vlen_bits.is_power_of_two());
        assert!(matches!(elen_bits, 32 | 64));
        assert!(instruction_bytes > 0 && instruction_bytes.is_multiple_of(4));
        Self {
            x: [0; 32],
            f: [0; 32],
            fcsr: 0,
            vector: Vector::new(vlen_bits, elen_bits),
            programs: [vec![0; instruction_bytes], vec![0; instruction_bytes]],
            loaded: [0; 2],
            constants: [Vec::new(), Vec::new()],
            pc: 0,
            entry: 0,
            instruction: 0,
            cycles: 0,
            vrf_ready: 0,
            lanes: config.lane_number as usize,
            base_integer: config.base_integer,
            fp64: config.fp64.expect("explicit GP FP64 policy"),
            memory_ports: config.memory_ports as usize,
            bank_bytes,
            operand_cache: vec![None; 4 * (config.lane_number as usize).min(vlen_bits / elen_bits)],
            timing_memo: Default::default(),
            operand_cache_next: 0,
            operand_states: Vec::new(),
            operand_state_ids: Default::default(),
            operand_state: None,
            operand_stale: false,
            profile: profile_path.map(|path| InstructionProfile {
                path,
                counts: std::collections::BTreeMap::new(),
            }),
        }
    }

    pub fn write_profile(&self) {
        use std::fmt::Write;
        if let Some(profile) = &self.profile {
            let mut csv = String::from("entry,pc,instruction,retired_instructions,modeled_cycles\n");
            for ((entry, pc, word), (count, cycles)) in &profile.counts {
                writeln!(csv, "0x{entry:x},0x{pc:x},0x{word:08x},{count},{cycles}").unwrap();
            }
            std::fs::write(&profile.path, csv).expect("Cannot write GP instruction profile");
        }
    }

    pub fn load_program(&mut self, buffer: usize, bytes: &[u8]) {
        assert!(bytes.len().is_multiple_of(4));
        self.programs[buffer][..bytes.len()].copy_from_slice(bytes);
        self.loaded[buffer] = bytes.len();
    }

    pub fn release_program(&mut self, buffer: usize) {
        self.loaded[buffer] = 0;
        self.constants[buffer].clear();
    }

    pub fn load_constants(&mut self, buffer: usize, bytes: &[u8]) {
        assert!(bytes.len() <= 4096);
        self.constants[buffer] = bytes.to_vec();
    }

    pub fn fault(&self, cause: u32, value: u64) -> Fault {
        Fault {
            pc: self.pc,
            instruction: self.instruction,
            cause,
            value,
        }
    }

    pub fn code_address(&self, address: u64) -> Result<u32, Fault> {
        let pc = u32::try_from(address).map_err(|_| self.fault(1, address))?;
        if pc & 3 != 0 {
            return Err(self.fault(0, address));
        }
        Ok(pc)
    }

    pub fn illegal(&self) -> Fault {
        self.fault(2, self.instruction.into())
    }

    pub fn read(&self, memory: &mut impl Memory, address: u64, bytes: usize) -> Result<u64, Fault> {
        memory.read(address, bytes).map_err(|_| self.fault(5, address))
    }

    pub fn write(&self, memory: &mut impl Memory, address: u64, bytes: usize, value: u64) -> Result<(), Fault> {
        memory.write(address, bytes, value).map_err(|_| self.fault(7, address))
    }

    pub fn run(
        &mut self,
        instruction_buffer: usize,
        entry: u32,
        end: u32,
        args: [u64; 8],
        stack_top: u64,
        memory: &mut impl Memory,
    ) -> Result<(), Fault> {
        self.pc = entry;
        self.entry = entry;
        self.instruction = 0;
        if entry & 3 != 0 || end & 3 != 0 || entry >= end || end as usize > self.loaded[instruction_buffer] {
            return Err(self.fault(1, entry.into()));
        }
        self.x.fill(0);
        self.f.fill(0);
        self.fcsr = 0;
        self.cycles = 1;
        self.vrf_ready = 0;
        self.operand_cache.fill(None);
        self.operand_cache_next = 0;
        self.operand_state = None;
        self.operand_stale = false;
        self.vector.vl = 0;
        self.vector.vtype = 1 << 31;
        self.vector.vstart = 0;
        self.vector.registers.fill(0);
        self.x[1] = end.into();
        self.x[2] = stack_top;
        self.x[10..18].copy_from_slice(&args);
        let constants = self.constants[instruction_buffer].clone();
        let mut memory = OperatorMemory::new(memory, &constants);
        while self.pc != end {
            if self.pc & 3 != 0 {
                return Err(self.fault(0, self.pc.into()));
            }
            let offset = self.pc as usize;
            let bytes = self.programs[instruction_buffer]
                .get(offset..offset + 4)
                .filter(|_| offset + 4 <= self.loaded[instruction_buffer])
                .ok_or_else(|| self.fault(1, self.pc.into()))?;
            self.instruction = u32::from_le_bytes(bytes.try_into().unwrap());
            let before = self.cycles;
            let pc = self.pc;
            let instruction = self.instruction;
            // Register the instruction response before execution, as in the GP RTL.
            self.cycles += 1;
            self.cycles += self.instruction_cycles();
            let opcode = self.instruction & 0x7f;
            let funct = (self.instruction >> 12) & 7;
            match opcode {
                0x57 => {
                    self.vector_execute()?;
                    self.pc = self.pc.wrapping_add(4);
                }
                0x07 | 0x27 if matches!(funct, 0 | 5 | 6 | 7) => {
                    self.vector_memory(&mut memory)?;
                    self.pc = self.pc.wrapping_add(4);
                }
                _ => self.scalar_execute(&mut memory)?,
            }
            if let Some(profile) = &mut self.profile {
                let counter = profile.counts.entry((entry, pc, instruction)).or_default();
                counter.0 += 1;
                counter.1 += self.cycles - before;
            }
            self.x[0] = 0;
        }
        Ok(())
    }
}
