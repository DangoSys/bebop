use super::{btrace, itrace, mtrace};
use std::cell::Cell;
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::Path;

thread_local! {
    static CURRENT_TRACE: Cell<*mut TraceState> = const { Cell::new(std::ptr::null_mut()) };
}

#[derive(Default)]
pub struct TraceState {
    pub(super) itrace_file: Option<File>,
    pub(super) mtrace_file: Option<File>,
    pub(super) clk: u64,
    pub(super) btrace: btrace::BtraceState,
}

#[derive(Clone, Debug, Default)]
pub struct TraceConfig {
    pub itrace: bool,
    pub mtrace: bool,
    pub btrace: bool,
}

impl TraceConfig {
    pub fn new(itrace: bool, mtrace: bool) -> Self {
        Self {
            itrace,
            mtrace,
            btrace: true,
        }
    }
}

impl TraceState {
    pub fn new(log_dir: &Path, config: TraceConfig) -> io::Result<Self> {
        std::fs::create_dir_all(log_dir)?;
        Ok(Self {
            itrace_file: itrace::init(log_dir, config.itrace)?,
            mtrace_file: mtrace::init(log_dir, config.mtrace)?,
            btrace: btrace::init(log_dir, config.btrace)?,
            clk: 0,
        })
    }

    pub fn set_bemu_clk(&mut self, clk: u64) {
        self.clk = clk;
    }

    pub fn bemu_clk(&self) -> u64 {
        self.clk
    }

    pub(super) fn write_itrace(&mut self, json: &str) {
        write_ndjson(&mut self.itrace_file, json);
    }

    pub(super) fn write_mtrace(&mut self, json: &str) {
        write_ndjson(&mut self.mtrace_file, json);
    }
}

fn write_ndjson(file: &mut Option<File>, json: &str) {
    if let Some(file) = file.as_mut() {
        writeln!(file, "{}", json).ok();
        file.flush().ok();
    }
}

pub unsafe fn with_trace_ptr<R>(trace: *mut TraceState, f: impl FnOnce() -> R) -> R {
    struct TraceGuard {
        previous: *mut TraceState,
    }

    impl Drop for TraceGuard {
        fn drop(&mut self) {
            CURRENT_TRACE.with(|current| current.set(self.previous));
        }
    }

    CURRENT_TRACE.with(|current| {
        let _guard = TraceGuard {
            previous: current.replace(trace),
        };
        f()
    })
}

pub(super) fn with_current_trace(f: impl FnOnce(&mut TraceState)) {
    CURRENT_TRACE.with(|current| {
        let trace = current.get();
        if !trace.is_null() {
            f(unsafe { &mut *trace });
        }
    });
}

pub(super) fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().rev().map(|b| format!("{:02x}", b)).collect()
}
