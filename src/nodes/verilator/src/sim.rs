// Verilator simulation control

use crate::ffi::*;
use crate::mmio;
use std::ffi::CString;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);

pub fn setup_ctrlc_handler() {
    ctrlc::set_handler(move || {
        eprintln!("\nReceived Ctrl-C, stopping simulation...");
        SHOULD_EXIT.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");
}

pub struct Simulator {
    context: *mut VerilatorContext,
    top: *mut VerilatorTop,
    trace: *mut VerilatorTrace,
    sim_time: u64,
    coverage: bool,
}

impl Simulator {
    pub fn new(fst_path: Option<&Path>, coverage: bool, args: &[String]) -> io::Result<Self> {
        // SAFETY: FFI calls to Verilator C++ runtime. All raw pointers returned by
        // verilator_*_new are null-checked before use; ownership is transferred to
        // Simulator which frees them in Drop. CString args outlive the FFI call.
        unsafe {
            // Create context
            let context = verilator_context_new();
            if context.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Failed to create Verilator context",
                ));
            }

            // Pass command args (need to prepend program name for VPI)
            let mut all_args = vec!["bebop-verilator".to_string()];
            all_args.extend_from_slice(args);

            let c_args: Vec<CString> = all_args.iter().map(|s| CString::new(s.as_str()).unwrap()).collect();
            let c_argv: Vec<*const i8> = c_args.iter().map(|s| s.as_ptr()).collect();
            verilator_context_command_args(context, c_argv.len() as i32, c_argv.as_ptr());

            // Create top module
            let top = verilator_top_new(context);
            if top.is_null() {
                verilator_context_free(context);
                return Err(io::Error::new(io::ErrorKind::Other, "Failed to create top module"));
            }

            let trace = if let Some(fst_path) = fst_path {
                let trace = verilator_trace_new();
                if trace.is_null() {
                    verilator_top_free(top);
                    verilator_context_free(context);
                    return Err(io::Error::new(io::ErrorKind::Other, "Failed to create trace"));
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
                    return Err(io::Error::new(io::ErrorKind::Other, "Failed to open FST file"));
                }

                println!("Waveform will be saved to: {}", fst_path.display());
                trace
            } else {
                std::ptr::null_mut()
            };

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
        // SAFETY: self.top is valid (initialized in new(), freed in Drop); FFI calls
        // are setter functions with no aliasing requirements beyond a valid top pointer.
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
        // SAFETY: self.{top, context, trace} are valid (set in new(), freed in Drop);
        // FFI calls advance simulation time and write to the trace file.
        unsafe {
            verilator_top_eval(self.top);
            verilator_context_time_inc(self.context, 1);
            let time = verilator_context_time(self.context);
            if !self.trace.is_null() {
                verilator_trace_dump(self.trace, time);
            }
            self.sim_time += 1;
        }
    }

    pub fn exec_once(&mut self) -> bool {
        // SAFETY: self.{top, context, trace} are valid; FFI calls drive one clock cycle,
        // sample MMIO, and dump waveforms. SCU DPI-C callbacks are invoked from RTL but
        // their unsafety is encapsulated in their extern "C" declarations.
        unsafe {
            // Posedge: clock=1, eval (SCU DPI-C functions called automatically from RTL)
            verilator_top_set_clock(self.top, 1);
            verilator_top_eval(self.top);

            // Check if SCU triggered sim_exit (via DPI-C callbacks)
            let should_exit = mmio::should_exit();

            verilator_context_time_inc(self.context, 1);
            let time = verilator_context_time(self.context);
            if !self.trace.is_null() {
                verilator_trace_dump(self.trace, time);
            }
            self.sim_time += 1;

            // Negedge: clock=0
            verilator_top_set_clock(self.top, 0);
            self.step_and_dump();

            should_exit
        }
    }

    pub fn run_batch<F>(&mut self, mut poll: F)
    where
        F: FnMut(),
    {
        loop {
            poll();
            if SHOULD_EXIT.load(Ordering::SeqCst) {
                eprintln!("Simulation interrupted by user");
                break;
            }
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
        // SAFETY: self.{context, trace} are valid; final time advance + trace close.
        // verilator_context_coverage_write only called when coverage was enabled in new().
        unsafe {
            verilator_context_time_inc(self.context, 1);
            let time = verilator_context_time(self.context);
            if !self.trace.is_null() {
                verilator_trace_dump(self.trace, time);
                verilator_trace_close(self.trace);
            }

            if self.coverage {
                verilator_context_coverage_write(self.context);
            }
        }
    }
}

impl Drop for Simulator {
    fn drop(&mut self) {
        // SAFETY: free FFI resources owned by self; null-check guards against partial init
        // failures in new(). After this, the raw pointers must not be used again.
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
