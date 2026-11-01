use bebop_cycle_trace::CycleTrace;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

static TRACE: OnceLock<Mutex<CycleTrace>> = OnceLock::new();

pub fn init_cycle_trace(log_dir: &Path) {
    let trace = TRACE.get_or_init(|| Mutex::new(CycleTrace::new(log_dir)));
    *trace.lock().unwrap() = CycleTrace::new(log_dir);
}

pub fn finish_cycle_trace() -> Result<(), String> {
    TRACE.get().expect("cycle trace initialized").lock().unwrap().finish()
}

#[no_mangle]
pub extern "C" fn dpi_trace_write(channel: u32, word: u32, lo: u32, hi: u32) {
    TRACE
        .get()
        .expect("cycle trace initialized")
        .lock()
        .unwrap()
        .write(channel, word as usize, (u64::from(hi) << 32) | u64::from(lo))
        .expect("record cycle trace event");
}
