use crate::ffi::run_spike;

pub struct SpikeRunConfig<'a> {
    isa: &'static str,
    procs: usize,
    mem_mb: usize,
    elf_file: &'a str,
    disasm_log_file: Option<&'a str>,
    pk: bool,
}

impl<'a> SpikeRunConfig<'a> {
    pub fn new(elf_file: &'a str, disasm_log_file: &'a str, pk: bool) -> Self {
        Self {
            isa: "rv64gc",
            procs: 1,
            mem_mb: 2048,
            elf_file,
            disasm_log_file: Some(disasm_log_file),
            pk,
        }
    }
}

pub fn run_spike_config(config: SpikeRunConfig<'_>) -> Result<(), String> {
    run_spike(
        config.isa,
        config.procs,
        config.mem_mb,
        config.elf_file,
        config.disasm_log_file,
        config.pk,
    )
}
