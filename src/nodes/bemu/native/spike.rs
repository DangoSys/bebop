use crate::ffi::{create_spike, NativeSpike};
use crate::trace::TraceConfig;
use std::path::Path;

pub struct SpikeInstance {
    mem_mb: usize,
    native: NativeSpike,
}

impl SpikeInstance {
    pub fn new(log_dir: &Path, trace_config: TraceConfig) -> Result<Self, String> {
        let isa = "rv64gc_xbuckyball_zicclsm_zicntr_zihpm";
        let procs = 1;
        let disasm_log_file = log_dir.join("disasm.log");
        let disasm_log_file = disasm_log_file
            .to_str()
            .ok_or_else(|| "invalid log_dir path".to_string())?;
        let native = create_spike(isa, procs, disasm_log_file, log_dir, trace_config)?;

        Ok(Self { mem_mb: 2048, native })
    }

    pub fn load_elf(&mut self, elf_file: &str) -> Result<(), String> {
        self.native.load_elf(elf_file)
    }

    pub fn init_hart(&mut self, pk: bool) -> Result<(), String> {
        self.native.init_hart(self.mem_mb, pk)
    }

    pub fn step(&mut self) -> Result<(), String> {
        self.native.step()
    }

    pub fn finished(&self) -> bool {
        self.native.finished()
    }

    pub fn exit_code(&self) -> Option<i32> {
        Some(self.native.exit_code())
    }

    pub fn total_latency(&self) -> u64 {
        self.native.total_latency()
    }
}
