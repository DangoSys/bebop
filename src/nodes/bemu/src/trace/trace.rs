use super::{banktrace, btrace, itrace, mtrace};
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

static TRACE: OnceLock<Mutex<TraceState>> = OnceLock::new();

#[derive(Default)]
pub(super) struct TraceState {
    pub(super) itrace_file: Option<File>,
    pub(super) mtrace_file: Option<File>,
    pub(super) banktrace_file: Option<File>,
    pub(super) clk: u64,
    pub(super) btrace: btrace::BtraceState,
}

#[derive(Clone, Debug, Default)]
pub struct TraceConfig {
    pub itrace: bool,
    pub mtrace: bool,
    pub banktrace: bool,
    pub btrace: bool,
}

impl TraceConfig {
    pub fn new(itrace: bool, mtrace: bool, banktrace: bool) -> Self {
        Self {
            itrace,
            mtrace,
            banktrace,
            btrace: true,
        }
    }
}

pub(super) fn trace_state() -> &'static Mutex<TraceState> {
    TRACE.get_or_init(|| Mutex::new(TraceState::default()))
}

pub fn init_trace(log_dir: &Path, config: TraceConfig) -> io::Result<()> {
    std::fs::create_dir_all(log_dir)?;
    let itrace_file = itrace::init(log_dir, config.itrace)?;
    let mtrace_file = mtrace::init(log_dir, config.mtrace)?;
    let banktrace_file = banktrace::init(log_dir, config.banktrace)?;
    let btrace = btrace::init(log_dir, config.btrace)?;

    let mut state = trace_state().lock().unwrap();
    state.itrace_file = itrace_file;
    state.mtrace_file = mtrace_file;
    state.banktrace_file = banktrace_file;
    state.btrace = btrace;
    Ok(())
}

pub fn shutdown_trace() -> io::Result<()> {
    btrace::shutdown()
}

pub fn set_bemu_clk(clk: u64) {
    trace_state().lock().unwrap().clk = clk;
}

pub fn bemu_clk() -> u64 {
    trace_state().lock().unwrap().clk
}

fn write_ndjson(file: &mut Option<File>, json: &str) {
    if let Some(file) = file.as_mut() {
        writeln!(file, "{}", json).ok();
        file.flush().ok();
    }
}

pub(super) fn write_itrace(json: &str) {
    write_ndjson(&mut trace_state().lock().unwrap().itrace_file, json);
}

pub(super) fn write_mtrace(json: &str) {
    write_ndjson(&mut trace_state().lock().unwrap().mtrace_file, json);
}

pub(super) fn write_banktrace(json: &str) {
    write_ndjson(&mut trace_state().lock().unwrap().banktrace_file, json);
}

pub(super) fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().rev().map(|b| format!("{:02x}", b)).collect()
}
