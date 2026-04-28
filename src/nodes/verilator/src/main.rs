use std::path::PathBuf;
use snafu::{Whatever, FromString};

use crate::{config, dram, mmio, sim, trace};

#[derive(Debug, Clone)]
pub struct VerilatorCli {
    pub elf: PathBuf,
    pub args: Vec<String>,
}

pub fn run(cli: VerilatorCli) -> Result<(), Whatever> {
    let config = VerilatorConfig::parse(cli)?;
    config.run()
}

#[derive(Debug, Clone)]
struct VerilatorConfig {
    elf: PathBuf,
    log: PathBuf,
    fst: PathBuf,
    stdout: Option<PathBuf>,
    trace_config: trace::TraceConfig,
    coverage: bool,
    mem_base: u64,
    mem_size: usize,
}

impl VerilatorConfig {
    fn parse(cli: VerilatorCli) -> Result<Self, Whatever> {
        let (log, fst, itrace, mtrace, pmctrace, ctrace, banktrace) =
            config::parse_args(cli.args)?;

        // Check for coverage env var
        let coverage = std::env::var("BEBOP_VERILATOR_COVERAGE")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        // Default memory config (can be made configurable)
        let mem_base = 0x8000_0000;
        let mem_size = 256 * 1024 * 1024; // 256MB

        Ok(Self {
            elf: cli.elf,
            log: PathBuf::from(log),
            fst: PathBuf::from(fst),
            stdout: None,
            trace_config: trace::TraceConfig {
                itrace,
                mtrace,
                pmctrace,
                ctrace,
                banktrace,
            },
            coverage,
            mem_base,
            mem_size,
        })
    }

    fn run(self) -> Result<(), Whatever> {
        // Initialize trace logging
        trace::init_trace(&self.log, self.trace_config.clone())
            .map_err(|e| Whatever::without_source(format!("Failed to init trace: {}", e)))?;

        println!("NDJSON trace: {}", self.log.display());
        println!(
            "Trace enabled: [itrace={} mtrace={} pmctrace={} ctrace={} banktrace={}]",
            self.trace_config.itrace,
            self.trace_config.mtrace,
            self.trace_config.pmctrace,
            self.trace_config.ctrace,
            self.trace_config.banktrace,
        );

        // Initialize UART
        mmio::init_uart(self.stdout.as_deref())
            .map_err(|e| Whatever::without_source(format!("Failed to init UART: {}", e)))?;

        // Initialize memory
        dram::init_memory(self.mem_base, self.mem_size)
            .map_err(|e| Whatever::without_source(format!("Failed to init memory: {}", e)))?;

        // Load ELF
        dram::load_elf(&self.elf, self.mem_base, self.mem_size)
            .map_err(|e| Whatever::without_source(format!("Failed to load ELF: {}", e)))?;

        // Create simulator
        let mut simulator = sim::Simulator::new(&self.fst, self.coverage, &[])
            .map_err(|e| Whatever::without_source(format!("Failed to create simulator: {}", e)))?;

        // Run simulation
        simulator.run_batch();

        // Finalize
        simulator.finalize();
        println!("Waveform saved to: {}", self.fst.display());

        Ok(())
    }
}

