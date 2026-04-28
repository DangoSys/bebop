// Verilator simulation control

use crate::ffi::*;
use crate::mmio;
use std::ffi::CString;
use std::io;
use std::path::Path;
use std::ptr;

pub struct Simulator {
    context: *mut VerilatorContext,
    top: *mut VerilatorTop,
    trace: *mut VerilatorTrace,
    sim_time: u64,
    coverage: bool,
}

impl Simulator {
    pub fn new(
        fst_path: &Path,
        coverage: bool,
        args: &[String],
    ) -> io::Result<Self> {
        unsafe {
            // Create context
            let context = verilator_context_new();
            if context.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to create Verilator context",
                ));
            }

            // Pass command args
            let c_args: Vec<CString> = args
                .iter()
                .map(|s| CString::new(s.as_str()).unwrap())
                .collect();
            let c_argv: Vec<*const i8> = c_args.iter().map(|s| s.as_ptr()).collect();
            verilator_context_command_args(context, c_argv.len() as i32, c_argv.as_ptr());

            // Create trace
            let trace = verilator_trace_new();
            if trace.is_null() {
                verilator_context_free(context);
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to create trace",
                ));
            }

            // Create top module
            let top = verilator_top_new(context);
            if top.is_null() {
                verilator_trace_free(trace);
                verilator_context_free(context);
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to create top module",
                ));
            }

            // Enable tracing
            verilator_context_trace_ever_on(context, true);
            verilator_top_trace(top, trace, 0);

            // Open FST file
            let fst_cstr = CString::new(fst_path.to_str().unwrap()).unwrap();
            if !verilator_trace_open(trace, fst_cstr.as_ptr()) {
                verilator_top_free(top);
                verilator_trace_free(trace);
                verilator_context_free(context);
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to open FST file",
                ));
            }

            println!("Waveform will be saved to: {}", fst_path.display());

            let mut sim = Simulator {
                context,
                top,
                trace,
                sim_time: 0,
                coverage,
            };

            // Reset sequence
            sim.reset();

            // Setup coverage signal handlers if enabled
            if coverage {
                sim.setup_coverage_handlers();
            }

            Ok(sim)
        }
    }

    fn reset(&mut self) {
        unsafe {
            // Reset high, clock low
            verilator_top_set_reset(self.top, 1);
            verilator_top_set_clock(self.top, 0);
            self.step_and_dump();

            // Reset high, clock high
            verilator_top_set_reset(self.top, 1);
            verilator_top_set_clock(self.top, 1);
            self.step_and_dump();

            // Reset low, clock low
            verilator_top_set_reset(self.top, 0);
            verilator_top_set_clock(self.top, 0);
            self.step_and_dump();
        }
    }

    fn step_and_dump(&mut self) {
        unsafe {
            verilator_top_eval(self.top);
            verilator_context_time_inc(self.context, 1);
            let time = verilator_context_time(self.context);
            verilator_trace_dump(self.trace, time);
            self.sim_time += 1;
        }
    }

    pub fn exec_once(&mut self) -> bool {
        unsafe {
            // Posedge: clock=1, eval, check MMIO
            verilator_top_set_clock(self.top, 1);
            verilator_top_eval(self.top);

            // Check MMIO (returns true if sim should exit)
            let should_exit = mmio::mmio_tick(self.top);

            verilator_context_time_inc(self.context, 1);
            let time = verilator_context_time(self.context);
            verilator_trace_dump(self.trace, time);
            self.sim_time += 1;

            // Negedge: clock=0
            verilator_top_set_clock(self.top, 0);
            self.step_and_dump();

            should_exit
        }
    }

    pub fn run_batch(&mut self) {
        loop {
            if self.exec_once() {
                break;
            }
        }
    }

    fn setup_coverage_handlers(&self) {
        // Setup signal handlers for coverage
        // In Rust, we can use ctrlc crate or signal-hook
        // For now, we'll handle this in the Drop impl
    }

    pub fn finalize(&mut self) {
        unsafe {
            verilator_context_time_inc(self.context, 1);
            let time = verilator_context_time(self.context);
            verilator_trace_dump(self.trace, time);
            verilator_trace_close(self.trace);

            if self.coverage {
                verilator_context_coverage_write(self.context);
            }
        }
    }
}

impl Drop for Simulator {
    fn drop(&mut self) {
        unsafe {
            if !self.top.is_null() {
                verilator_top_free(self.top);
            }
            if !self.trace.is_null() {
                verilator_trace_free(self.trace);
            }
            if !self.context.is_null() {
                verilator_context_free(self.context);
            }
        }
    }
}
