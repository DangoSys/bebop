use crate::emu::bemu::Bemu;
use crate::emu::config::BemuStats;
use log::debug;

pub type SpikeResult = Result<u64, SpikeError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpikeError {
    InvalidMemoryAccess(u64),
}

impl std::fmt::Display for SpikeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpikeError::InvalidMemoryAccess(addr) => {
                write!(f, "Invalid memory access: addr=0x{addr:x}")
            }
        }
    }
}

impl std::error::Error for SpikeError {}

#[derive(Debug, Clone)]
pub struct SpikeCallbackParams {
    pub funct: u32,
    pub xs1: u64,
    pub xs2: u64,
    pub pc: Option<u64>,
}

impl SpikeCallbackParams {
    pub fn new(funct: u32, xs1: u64, xs2: u64) -> Self {
        Self {
            funct,
            xs1,
            xs2,
            pc: None,
        }
    }
}

pub struct BemuSpikeInterface {
    bemu: Bemu,
    verbose: bool,
    instruction_count: u64,
}

impl BemuSpikeInterface {
    pub fn new() -> Self {
        Self {
            bemu: Bemu::new(),
            verbose: false,
            instruction_count: 0,
        }
    }

    pub fn with_verbose(verbose: bool) -> Self {
        let mut bemu = Bemu::new();
        bemu.set_verbose(verbose);
        Self {
            bemu,
            verbose,
            instruction_count: 0,
        }
    }

    pub fn handle_custom_instruction(&mut self, params: &SpikeCallbackParams) -> SpikeResult {
        self.instruction_count += 1;
        if self.verbose {
            debug!(
                "funct={}, xs1=0x{:x}, xs2=0x{:x}, pc={:?}",
                params.funct, params.xs1, params.xs2, params.pc
            );
        }
        Ok(self.bemu.execute(params.funct, params.xs1, params.xs2))
    }

    pub fn sync_memory(&mut self, addr: u64, data: &[u8]) -> Result<(), SpikeError> {
        if self.verbose {
            debug!("sync addr=0x{:x}, size={}", addr, data.len());
        }
        self.bemu.write_memory(addr, data);
        Ok(())
    }

    pub fn read_memory(&self, addr: u64, size: usize) -> Result<Vec<u8>, SpikeError> {
        let _ = addr
            .checked_add(size as u64)
            .ok_or(SpikeError::InvalidMemoryAccess(addr))?;
        Ok(self.bemu.read_memory(addr, size))
    }

    pub fn get_stats(&self) -> &BemuStats {
        self.bemu.get_stats()
    }

    pub fn reset_stats(&mut self) {
        self.bemu.reset_stats();
        self.instruction_count = 0;
    }
}

impl Default for BemuSpikeInterface {
    fn default() -> Self {
        Self::new()
    }
}
