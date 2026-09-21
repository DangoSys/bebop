use super::trace::with_current_trace;
use bebop_bank_hash::{observe, BTraceBank, BTraceRecord, BTraceSource, BTraceTime};
use std::fs::{File, OpenOptions};
use std::io;
use std::io::Write;
use std::path::Path;

const GOLDEN_RECORD_FILE: &str = "bemu_btrace.ndjson";

#[derive(Debug, Default)]
pub(super) struct BtraceState {
    next_line: u64,
    golden_record_file: Option<File>,
}

impl BtraceState {
    pub(super) fn enabled(&self) -> bool {
        self.golden_record_file.is_some()
    }

    fn next_line(&mut self) -> u64 {
        self.next_line = self.next_line.wrapping_add(1);
        self.next_line
    }
}

pub(super) fn init(log_dir: &Path, enabled: bool) -> io::Result<BtraceState> {
    if !enabled {
        return Ok(BtraceState::default());
    }

    let golden_record_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_dir.join(GOLDEN_RECORD_FILE))?;
    Ok(BtraceState {
        golden_record_file: Some(golden_record_file),
        ..BtraceState::default()
    })
}

#[allow(clippy::too_many_arguments)]
pub fn bemu_btrace(
    inst_id: u64,
    hart_id: u64,
    r0: BTraceBank,
    r1: BTraceBank,
    w0: BTraceBank,
    funct7: u32,
    op_type: &str,
    pc: u64,
) {
    with_current_trace(|trace| {
        let line_number = trace.btrace.next_line();
        let record = BTraceRecord::new(
            BTraceSource::Bemu,
            inst_id,
            hart_id,
            r0,
            r1,
            w0,
            funct7,
            op_type,
            BTraceTime::Cycle(trace.bemu_clk()),
            Some(pc),
            Some(format!("{GOLDEN_RECORD_FILE}:{line_number}")),
        );

        if let (Some(file), Ok(line)) = (trace.btrace.golden_record_file.as_mut(), record.to_ndjson()) {
            file.write_all(line.as_bytes()).ok();
            file.flush().ok();
        }
        observe(&record);
    });
}
