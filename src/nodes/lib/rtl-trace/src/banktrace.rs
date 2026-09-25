use bebop_bank_hash::{observe, BTraceBank, BTraceRecord, BTraceSource, BTraceTime};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use crate::state;

static OUTPUT: OnceLock<Mutex<Option<File>>> = OnceLock::new();

pub fn init(log_dir: &Path, enabled: bool) -> io::Result<()> {
    let output = enabled
        .then(|| {
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(log_dir.join("rtl_btrace.ndjson"))
        })
        .transpose()?;
    *OUTPUT.get_or_init(|| Mutex::new(None)).lock().unwrap() = output;
    Ok(())
}

pub fn btrace(inst_id: u64, hart_id: u64, w0: BTraceBank) {
    let record = BTraceRecord::new(
        BTraceSource::Rtl,
        inst_id,
        hart_id,
        w0,
        0,
        "btrace".to_string(),
        BTraceTime::Cycle(state::rtl_clk()),
        None,
        None,
    );
    if let Some(output) = OUTPUT.get_or_init(|| Mutex::new(None)).lock().unwrap().as_mut() {
        write!(output, "{}", record.to_ndjson().unwrap()).unwrap();
        output.flush().unwrap();
    }
    observe(&record);
}
