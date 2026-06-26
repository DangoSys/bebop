//===------ run.rs ---------- Verilator simulation runner ----------------===//
//
// Copyright 2026 The Aerospace Corporation
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
//===----------------------------------------------------------------------===//
//
// TODO: support Verilator diff/fast mode.
//
//===----------------------------------------------------------------------===//
use snafu::{FromString, Whatever};
use std::path::PathBuf;

#[cfg(feature = "verilator")]
use snafu::ResultExt;
#[cfg(feature = "verilator")]
use std::fs::File;
#[cfg(feature = "verilator")]
use std::io::{BufReader, BufWriter};
#[cfg(feature = "verilator")]
use std::os::fd::AsRawFd;
#[cfg(feature = "verilator")]
use std::path::Path;

#[cfg(feature = "verilator")]
use bebop_fd_redirect::FdRedirect;

#[cfg(feature = "verilator")]
use bebop_verilator::{exit_code, init_trace, setup_ctrlc_handler, should_exit, Simulator, TraceConfig};

#[cfg(feature = "verilator")]
use super::console::ConsoleServer;

pub struct VerilatorRunConfig {
    pub elf: PathBuf,
    pub log_dir: PathBuf,
    pub fst_dir: Option<PathBuf>,
    pub wave: bool,
    pub diff: bool,
    pub fast: bool,
    pub trace: VerilatorTraceConfig,
}

#[derive(Debug)]
pub struct VerilatorTraceConfig {
    pub itrace: bool,
    pub mtrace: bool,
    pub pmctrace: bool,
    pub ctrace: bool,
    pub banktrace: bool,
}

impl VerilatorRunConfig {
    fn mode(&self) -> &'static str {
        if self.fast && self.diff {
            "fast+diff"
        } else if self.fast {
            "fast"
        } else if self.diff {
            "diff"
        } else {
            "run"
        }
    }
}

pub fn run(config: VerilatorRunConfig) -> Result<(), Whatever> {
    #[cfg(feature = "verilator")]
    {
        //===----------------------------------------------------------------------===//
        // Configuration Checks
        //===----------------------------------------------------------------------===//
        setup_ctrlc_handler();
        if config.diff || config.fast {
            return Err(Whatever::without_source(
                "Verilator diff/fast run is not supported yet".to_string(),
            ));
        }

        let stdout_file = config.log_dir.join("stdout.log");
        let stderr_file = config.log_dir.join("stderr.log");
        let fst_file = config
            .fst_dir
            .as_ref()
            .cloned()
            .unwrap_or_else(|| config.log_dir.join("fst"))
            .join("waveform.fst");
        let trace_config = TraceConfig {
            itrace: config.trace.itrace,
            mtrace: config.trace.mtrace,
            pmctrace: config.trace.pmctrace,
            ctrace: config.trace.ctrace,
            banktrace: config.trace.banktrace,
        };

        println!("ELF file: {}", config.elf.display());
        println!("Simulator mode: {}", config.mode());
        println!("Trace configuration: {:?}", config.trace);
        println!("Log directory: {}", config.log_dir.display());
        if let Some(fst_dir) = config.fst_dir.as_ref() {
            println!("Waveform will be saved to: {}", fst_dir.display());
        }

        create_output_dirs(&config.log_dir, config.wave.then_some(fst_file.as_path()))?;
        init_trace(&config.log_dir, trace_config)
            .map_err(|e| Whatever::without_source(format!("failed to init Verilator trace: {e}")))?;

        //===----------------------------------------------------------------------===//
        // Initialize Verilator
        //===----------------------------------------------------------------------===//
        let stdout_guard = FdRedirect::new_tee(std::io::stdout().as_raw_fd(), &stdout_file, "stdout")
            .whatever_context("failed to redirect stdout")?;
        let stderr_guard = FdRedirect::new(std::io::stderr().as_raw_fd(), &stderr_file, "stderr")
            .whatever_context("failed to redirect stderr")?;

        let console = ConsoleServer::start(&config.log_dir)?;
        println!("Console socket: {}", console.socket_path().display());
        println!("UART logs: {}", console.uart_log_dir().display());

        let elf_arg = format!("+elf={}", config.elf.display());
        let mut simulator = Simulator::new(config.wave.then_some(fst_file.as_path()), &[elf_arg])
            .map_err(|e| Whatever::without_source(format!("failed to create Verilator simulator: {e}")))?;

        //===----------------------------------------------------------------------===//
        // Run
        //===----------------------------------------------------------------------===//
        loop {
            console.poll_tx();
            if simulator.exec_once() {
                break;
            }
            if should_exit() {
                break;
            }
        }
        console.poll_tx();
        let code = exit_code();

        //===----------------------------------------------------------------------===//
        // Finish Simulation
        //===----------------------------------------------------------------------===//
        simulator.finalize();

        drop(console);
        drop(stderr_guard);
        drop(stdout_guard);

        write_disasm_log(&stderr_file)?;

        if code != 0 {
            return Err(Whatever::without_source(format!("Verilator exited with code {code}")));
        }
        Ok(())
    }

    #[cfg(not(feature = "verilator"))]
    {
        let _ = config;
        Err(Whatever::without_source(
            "verilator runner is not compiled into this executable".to_string(),
        ))
    }
}

#[cfg(feature = "verilator")]
fn create_output_dirs(log_dir: &Path, fst_file: Option<&Path>) -> Result<(), Whatever> {
    std::fs::create_dir_all(log_dir).whatever_context("failed to create Verilator log dir")?;
    if let Some(parent) = fst_file.and_then(Path::parent) {
        std::fs::create_dir_all(parent).whatever_context("failed to create Verilator fst dir")?;
    }
    Ok(())
}

#[cfg(feature = "verilator")]
fn write_disasm_log(stderr_file: &Path) -> Result<(), Whatever> {
    let disasm_file = stderr_file.with_file_name("disasm.log");
    let input = File::open(stderr_file)
        .map_err(|e| Whatever::without_source(format!("failed to open stderr.log for disasm: {e}")))?;
    let output = File::create(&disasm_file)
        .map_err(|e| Whatever::without_source(format!("failed to create disasm.log: {e}")))?;

    if let Err(e) = bebop_dasm::process_dasm(BufReader::new(input), BufWriter::new(output)) {
        eprintln!("Warning: failed to disassemble Verilator stderr: {e}");
    } else {
        println!("Disassembly saved to: {}", disasm_file.display());
    }
    Ok(())
}
