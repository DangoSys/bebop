use snafu::{FromString, Whatever};
use std::fs::{File, OpenOptions};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::PathBuf;

use crate::{mmio, sim, trace};

#[derive(Debug, Clone)]
pub struct VerilatorCli {
    pub elf: PathBuf,
    pub log_dir: PathBuf,
    pub fst_dir: PathBuf,
    pub itrace: bool,
    pub mtrace: bool,
    pub pmctrace: bool,
    pub ctrace: bool,
    pub banktrace: bool,
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
        // Check for coverage env var
        let coverage = std::env::var("BEBOP_VERILATOR_COVERAGE")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        // Default memory config (can be made configurable)
        let mem_base = 0x8000_0000;
        let mem_size = 256 * 1024 * 1024; // 256MB

        // Generate file paths from directories
        let log = cli.log_dir.join("bdb.ndjson");
        let stdout = Some(cli.log_dir.join("stdout.log"));
        let fst = cli.fst_dir.join("waveform.fst");

        Ok(Self {
            elf: cli.elf,
            log,
            fst,
            stdout,
            trace_config: trace::TraceConfig {
                itrace: cli.itrace,
                mtrace: cli.mtrace,
                pmctrace: cli.pmctrace,
                ctrace: cli.ctrace,
                banktrace: cli.banktrace,
            },
            coverage,
            mem_base,
            mem_size,
        })
    }

    fn run(self) -> Result<(), Whatever> {
        // Setup Ctrl-C handler
        sim::setup_ctrlc_handler();

        // Create fst directory if needed
        if let Some(parent) = self.fst.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Whatever::without_source(format!("Failed to create fst directory: {}", e)))?;
        }

        // Redirect stderr to stdout.log if specified
        let _stderr_guard = if let Some(ref stdout_path) = self.stdout {
            // Create parent directory if needed
            if let Some(parent) = stdout_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| Whatever::without_source(format!("Failed to create log directory: {}", e)))?;
            }

            // Open the stdout log file
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(stdout_path)
                .map_err(|e| Whatever::without_source(format!("Failed to open stdout log: {}", e)))?;

            // Redirect stderr to the file
            let stderr_fd = std::io::stderr().as_raw_fd();
            let file_fd = file.as_raw_fd();

            unsafe {
                let old_stderr = libc::dup(stderr_fd);
                if old_stderr < 0 {
                    return Err(Whatever::without_source("Failed to duplicate stderr".to_string()));
                }
                if libc::dup2(file_fd, stderr_fd) < 0 {
                    libc::close(old_stderr);
                    return Err(Whatever::without_source("Failed to redirect stderr".to_string()));
                }

                // Return a guard that will restore stderr on drop
                Some((file, old_stderr))
            }
        } else {
            None
        };

        // Initialize trace logging
        trace::init_trace(&self.log, self.trace_config.clone())
            .map_err(|e| Whatever::without_source(format!("Failed to init trace: {}", e)))?;

        println!("NDJSON trace: {}", self.log.display());
        if let Some(ref stdout_path) = self.stdout {
            println!("Stdout log: {}", stdout_path.display());
        }
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

        // Create simulator with +elf= argument for BBSimDRAM
        let elf_arg = format!("+elf={}", self.elf.display());
        let mut simulator = sim::Simulator::new(&self.fst, self.coverage, &[elf_arg])
            .map_err(|e| Whatever::without_source(format!("Failed to create simulator: {}", e)))?;

        // Run simulation
        simulator.run_batch();

        // Finalize
        simulator.finalize();
        println!("Waveform saved to: {}", self.fst.display());

        // Restore stderr if it was redirected
        if let Some((_, old_stderr)) = _stderr_guard {
            unsafe {
                libc::dup2(old_stderr, std::io::stderr().as_raw_fd());
                libc::close(old_stderr);
            }
        }

        // Run disassembler on stdout.log to generate disasm.log
        if let Some(ref stdout_path) = self.stdout {
            let disasm_path = stdout_path.with_file_name("disasm.log");
            let stdin_file = std::fs::File::open(stdout_path)
                .map_err(|e| Whatever::without_source(format!("Failed to open stdout.log: {}", e)))?;
            let stdout_file = std::fs::File::create(&disasm_path)
                .map_err(|e| Whatever::without_source(format!("Failed to create disasm.log: {}", e)))?;

            let reader = std::io::BufReader::new(stdin_file);
            let writer = std::io::BufWriter::new(stdout_file);

            if let Err(e) = bebop_dasm::process_dasm(reader, writer) {
                eprintln!("Warning: Failed to disassemble: {}", e);
            } else {
                println!("Disassembly saved to: {}", disasm_path.display());
            }
        }

        Ok(())
    }
}
