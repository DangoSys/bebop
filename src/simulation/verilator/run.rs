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
// Diff mode runs the RTL and single-core BEMU command stream together while a
// background worker pairs stable whole-Bank events.
//
//===----------------------------------------------------------------------===//
use snafu::{FromString, Whatever};
#[cfg(feature = "verilator")]
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
use bebop_verilator::{
    exit_code, init_trace, rtl_bank_difftest_status, setup_ctrlc_handler, should_exit, Simulator, TraceConfig,
};

#[cfg(all(feature = "verilator", feature = "bemu"))]
use bebop_bank_hash::{
    init_runtime_packet_channel, run_online_compare_with_summary, runtime_bank_difftest_failure_detected,
    shutdown_runtime_packet_channel, BankHashCompareSummary,
};
#[cfg(all(feature = "verilator", feature = "bemu"))]
use bebop_bemu::{BemuInstance, TraceConfig as BemuTraceConfig};
#[cfg(all(feature = "verilator", feature = "bemu"))]
use std::thread::JoinHandle;

#[cfg(feature = "verilator")]
use super::console::ConsoleServer;

#[cfg(feature = "verilator")]
pub struct VerilatorRunConfig {
    pub elf: PathBuf,
    pub log_dir: PathBuf,
    pub fst_dir: Option<PathBuf>,
    pub wave: bool,
    pub diff: bool,
    pub fast: bool,
    pub trace: VerilatorTraceConfig,
    pub fault: Option<SpmFaultConfig>,
}

#[derive(Clone, Copy, Debug)]
#[cfg(feature = "verilator")]
pub struct SpmFaultConfig {
    pub semantic_seq: Option<u64>,
    pub byte_offset: u32,
    pub bit: u8,
}

#[derive(Debug)]
#[cfg(feature = "verilator")]
pub struct VerilatorTraceConfig {
    pub itrace: bool,
    pub mtrace: bool,
    pub pmctrace: bool,
    pub ctrace: bool,
    pub banktrace: bool,
}

#[cfg(feature = "verilator")]
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

#[cfg(feature = "verilator")]
pub fn run(config: VerilatorRunConfig) -> Result<(), Whatever> {
    //===----------------------------------------------------------------------===//
    // Configuration Checks
    //===----------------------------------------------------------------------===//
    setup_ctrlc_handler();
    if config.fast {
        return Err(Whatever::without_source(
            "Verilator fast run is not supported yet".to_string(),
        ));
    }
    #[cfg(not(feature = "bemu"))]
    if config.diff {
        return Err(Whatever::without_source(
            "this executable was built without BEMU; rebuild Verilator with --diff".to_string(),
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
        bank_hash: config.diff,
        spm_fault: config.fault.map(|fault| bebop_verilator::SpmFaultConfig {
            semantic_seq: fault.semantic_seq,
            byte_offset: fault.byte_offset,
            bit: fault.bit,
        }),
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

    #[cfg(feature = "bemu")]
    let mut diff_session = if config.diff {
        let (rtl_bank_count, rtl_bank_size) = simulator
            .private_bank_layout()
            .map_err(|e| Whatever::without_source(format!("failed to query RTL Bank model: {e}")))?;
        Some(DiffSession::new(
            &config.elf,
            &config.log_dir,
            rtl_bank_count,
            rtl_bank_size,
        )?)
    } else {
        None
    };

    //===----------------------------------------------------------------------===//
    // Run
    //===----------------------------------------------------------------------===//
    #[cfg(feature = "bemu")]
    let mut stopped_on_bank_difftest_failure = false;
    loop {
        console.poll_tx();
        let rtl_outcome = simulator.exec_once();
        #[cfg(feature = "bemu")]
        if rtl_outcome.rtl_difftest_failure || (config.diff && runtime_bank_difftest_failure_detected()) {
            stopped_on_bank_difftest_failure = true;
            println!("Bank DiffTest failure detected; stopping RTL simulation immediately");
            break;
        }
        if rtl_outcome.finished {
            break;
        }
        #[cfg(feature = "bemu")]
        if let Some(diff) = diff_session.as_mut() {
            diff.step_golden()?;
        }
        #[cfg(feature = "bemu")]
        if config.diff && runtime_bank_difftest_failure_detected() {
            stopped_on_bank_difftest_failure = true;
            println!("Bank DiffTest failure detected; stopping RTL simulation immediately");
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

    let rtl_diff_status = if config.diff {
        let status = rtl_bank_difftest_status();
        #[cfg(feature = "bemu")]
        let require_drained = !stopped_on_bank_difftest_failure;
        #[cfg(not(feature = "bemu"))]
        let require_drained = true;
        if require_drained && !status.is_drained() {
            return Err(Whatever::without_source(format!(
                "RTL Bank DiffTest did not drain: in_flight_operations={} in_flight_bank_writers={} pending_stable_boundaries={}",
                status.in_flight_operations, status.in_flight_bank_writers, status.pending_stable_boundaries
            )));
        }
        Some(status)
    } else {
        None
    };

    #[cfg(feature = "bemu")]
    let diff_summary = if let Some(mut diff) = diff_session {
        if stopped_on_bank_difftest_failure {
            Some(diff.finish()?)
        } else {
            diff.finish_golden()?;
            Some(diff.finish()?)
        }
    } else {
        None
    };

    drop(console);
    drop(stderr_guard);
    drop(stdout_guard);

    write_disasm_log(&stderr_file)?;

    if let Some(status) = rtl_diff_status {
        println!(
            "SPM fault injection: requested={} injected={}",
            config.fault.is_some(),
            status.spm_fault_injected
        );
        println!(
            "RTL Bank target summary: checks={} mismatch={} attribution_errors={}",
            status.bank_target_checks, status.bank_target_mismatches, status.bank_write_attribution_errors
        );
        if !status.bank_targets_are_clean() {
            return Err(Whatever::without_source(format!(
                "RTL Bank target check failed: mismatch={} attribution_errors={}",
                status.bank_target_mismatches, status.bank_write_attribution_errors
            )));
        }
    }
    #[cfg(feature = "bemu")]
    if let Some(summary) = diff_summary {
        println!(
            "Bank DiffTest summary: pass={} content_mismatch={} target_mismatch={} dependency_mismatch={} missing_rtl={} missing_golden={} protocol_errors={}",
            summary.pass,
            summary.mismatch,
            summary.target_mismatch,
            summary.dependency_mismatch,
            summary.missing_rtl,
            summary.missing_bemu,
            summary.protocol_errors
        );
        if !summary.is_clean() {
            return Err(Whatever::without_source(format!(
                "Bank DiffTest failed: content_mismatch={} target_mismatch={} dependency_mismatch={} missing_rtl={} missing_golden={} protocol_errors={}",
                summary.mismatch,
                summary.target_mismatch,
                summary.dependency_mismatch,
                summary.missing_rtl,
                summary.missing_bemu,
                summary.protocol_errors
            )));
        }
    }
    if code != 0 {
        return Err(Whatever::without_source(format!("Verilator exited with code {code}")));
    }
    Ok(())
}

#[cfg(all(feature = "verilator", feature = "bemu"))]
struct DiffSession {
    golden: BemuInstance,
    worker: Option<JoinHandle<Result<BankHashCompareSummary, String>>>,
}

#[cfg(all(feature = "verilator", feature = "bemu"))]
impl DiffSession {
    fn new(elf: &Path, log_dir: &Path, rtl_bank_count: usize, rtl_bank_size: usize) -> Result<Self, Whatever> {
        let golden_log_dir = log_dir.join("golden");
        let mut trace = BemuTraceConfig::new(false, false);
        trace.btrace = true;
        let mut golden =
            BemuInstance::new(&golden_log_dir, trace).whatever_context("failed to create BEMU Golden Model")?;
        let snapshot = golden.scratchpad_snapshot();
        let actual_bank_size = snapshot.first().map(Vec::len).unwrap_or(0);
        if snapshot.len() != rtl_bank_count
            || actual_bank_size != rtl_bank_size
            || snapshot.iter().any(|bank| bank.len() != actual_bank_size)
        {
            return Err(Whatever::without_source(format!(
                "BEMU/RTL Bank model mismatch: BEMU={} Banks x {} bytes, RTL={} Banks x {} bytes",
                snapshot.len(),
                actual_bank_size,
                rtl_bank_count,
                rtl_bank_size
            )));
        }
        golden.load_elf(elf)?;
        golden.init_hart(false)?;

        let receiver = init_runtime_packet_channel();
        let output = log_dir.join("bank_diff.ndjson");
        let worker = std::thread::Builder::new()
            .name("bank-diff-engine".to_string())
            .spawn(move || run_online_compare_with_summary(receiver, output).map_err(|error| error.to_string()))
            .map_err(|error| {
                shutdown_runtime_packet_channel();
                Whatever::without_source(format!("failed to start asynchronous Bank Diff Engine: {error}"))
            })?;
        Ok(Self {
            golden,
            worker: Some(worker),
        })
    }

    fn step_golden(&mut self) -> Result<(), Whatever> {
        if !self.golden.finished() {
            self.golden.step()?;
        }
        Ok(())
    }

    fn finish_golden(&mut self) -> Result<(), Whatever> {
        while !self.golden.finished() {
            self.golden.step()?;
        }
        let code = self.golden.exit_code().unwrap_or(0);
        if code != 0 {
            return Err(Whatever::without_source(format!(
                "BEMU Golden Model exited with code {code}"
            )));
        }
        Ok(())
    }

    fn finish(mut self) -> Result<BankHashCompareSummary, Whatever> {
        shutdown_runtime_packet_channel();
        self.worker
            .take()
            .expect("Diff worker exists")
            .join()
            .map_err(|_| Whatever::without_source("Bank Diff Engine panicked".to_string()))?
            .map_err(Whatever::without_source)
    }
}

#[cfg(all(feature = "verilator", feature = "bemu"))]
impl Drop for DiffSession {
    fn drop(&mut self) {
        shutdown_runtime_packet_channel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(not(feature = "verilator"))]
pub fn run_unavailable() -> Result<(), Whatever> {
    Err(Whatever::without_source(
        "verilator runner is not compiled into this executable".to_string(),
    ))
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
