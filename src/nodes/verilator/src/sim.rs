use crate::ffi::*;
use crate::mmio;
use crate::trace;
use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);

pub fn setup_ctrlc_handler() {
    ctrlc::set_handler(move || {
        eprintln!("Simulation interrupted");
        SHOULD_EXIT.store(true, Ordering::SeqCst);
    })
    .expect("failed to set Ctrl-C handler");
}

pub fn should_exit() -> bool {
    SHOULD_EXIT.load(Ordering::SeqCst)
}

pub struct Simulator {
    context: *mut VerilatorContext,
    top: *mut VerilatorTop,
    trace: *mut VerilatorTrace,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExecOutcome {
    pub finished: bool,
    pub rtl_bank_hash_events: u32,
    pub rtl_difftest_failure: bool,
    pub rtl_bank_hash_pending: bool,
}

impl Simulator {
    pub fn new(fst_path: Option<&Path>, args: &[String]) -> io::Result<Self> {
        unsafe {
            let context = verilator_context_new();
            if context.is_null() {
                return Err(io::Error::other("failed to create Verilator context"));
            }

            if let Err(e) = set_command_args(context, args) {
                verilator_context_free(context);
                return Err(e);
            }

            let top = verilator_top_new(context);
            if top.is_null() {
                verilator_context_free(context);
                return Err(io::Error::other("failed to create top module"));
            }
            trace::set_verilator_top(top);

            let trace = match init_waveform(context, top, fst_path) {
                Ok(trace) => trace,
                Err(e) => {
                    trace::set_verilator_top(std::ptr::null_mut());
                    verilator_top_free(top);
                    verilator_context_free(context);
                    return Err(e);
                }
            };

            let mut sim = Simulator { context, top, trace };

            sim.reset();
            Ok(sim)
        }
    }

    fn reset(&mut self) {
        unsafe {
            verilator_top_set_reset(self.top, 1);
            verilator_top_set_clock(self.top, 0);
            self.step_and_dump();

            verilator_top_set_reset(self.top, 1);
            verilator_top_set_clock(self.top, 1);
            self.step_and_dump();

            verilator_top_set_reset(self.top, 0);
            verilator_top_set_clock(self.top, 0);
            self.step_and_dump();
        }
    }

    fn step_and_dump(&mut self) -> trace::RtlBankHashEvalOutcome {
        unsafe {
            verilator_top_eval(self.top);
            let outcome = trace::finish_rtl_bank_hash_eval();
            verilator_context_time_inc(self.context, 1);
            let time = verilator_context_time(self.context);
            if !self.trace.is_null() {
                verilator_trace_dump(self.trace, time);
            }
            outcome
        }
    }

    pub fn exec_once(&mut self) -> ExecOutcome {
        unsafe {
            verilator_top_set_clock(self.top, 1);
            verilator_top_eval(self.top);
            let rising = trace::finish_rtl_bank_hash_eval();

            let should_exit = mmio::should_exit();

            verilator_context_time_inc(self.context, 1);
            let time = verilator_context_time(self.context);
            if !self.trace.is_null() {
                verilator_trace_dump(self.trace, time);
            }

            verilator_top_set_clock(self.top, 0);
            let falling = self.step_and_dump();

            ExecOutcome {
                finished: should_exit,
                rtl_bank_hash_events: rising.events.saturating_add(falling.events),
                rtl_difftest_failure: rising.failure || falling.failure,
                rtl_bank_hash_pending: falling.pending,
            }
        }
    }

    pub fn finalize(&mut self) {
        unsafe {
            verilator_context_time_inc(self.context, 1);
            let time = verilator_context_time(self.context);
            if !self.trace.is_null() {
                verilator_trace_dump(self.trace, time);
                verilator_trace_close(self.trace);
            }
        }
    }

    pub fn private_bank_layout(&self) -> io::Result<(usize, usize)> {
        let count = unsafe { verilator_private_bank_count() as usize };
        let bytes = unsafe { verilator_private_bank_bytes(self.top) as usize };
        if count == 0 || bytes == 0 {
            return Err(io::Error::other(
                "Verilator private-Bank layout backdoor is unavailable",
            ));
        }
        Ok((count, bytes))
    }
}

unsafe fn set_command_args(context: *mut VerilatorContext, args: &[String]) -> io::Result<()> {
    let mut all_args = vec!["bebop-verilator".to_string()];
    all_args.extend_from_slice(args);

    let c_args: Vec<CString> = all_args
        .iter()
        .map(|arg| CString::new(arg.as_str()).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e)))
        .collect::<io::Result<_>>()?;
    let c_argv: Vec<*const i8> = c_args.iter().map(|arg| arg.as_ptr()).collect();

    verilator_context_command_args(context, c_argv.len() as i32, c_argv.as_ptr());
    Ok(())
}

unsafe fn init_waveform(
    context: *mut VerilatorContext,
    top: *mut VerilatorTop,
    fst_path: Option<&Path>,
) -> io::Result<*mut VerilatorTrace> {
    let Some(fst_path) = fst_path else {
        return Ok(std::ptr::null_mut());
    };

    let trace = verilator_trace_new();
    if trace.is_null() {
        return Err(io::Error::other("failed to create trace"));
    }

    verilator_context_trace_ever_on(context, true);
    verilator_top_trace(top, trace, 0);

    let fst_cstr =
        CString::new(fst_path.as_os_str().as_bytes()).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    if !verilator_trace_open(trace, fst_cstr.as_ptr()) {
        verilator_trace_free(trace);
        return Err(io::Error::other("failed to open FST file"));
    }

    println!("Waveform will be saved to: {}", fst_path.display());
    Ok(trace)
}

impl Drop for Simulator {
    fn drop(&mut self) {
        unsafe {
            if !self.top.is_null() {
                trace::set_verilator_top(std::ptr::null_mut());
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
