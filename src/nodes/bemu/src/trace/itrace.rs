use super::trace::with_current_trace;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

const LOG_FILE: &str = "itrace.ndjson";

pub struct ITraceEvent {
    pub funct: u32,
    pub pc: u64,
    pub rs1: u64,
    pub rs2: u64,
}

pub(super) fn init(log_dir: &Path, enabled: bool) -> io::Result<Option<File>> {
    if !enabled {
        return Ok(None);
    }

    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_dir.join(LOG_FILE))
        .map(Some)
}

pub fn itrace(event: ITraceEvent) {
    with_current_trace(|trace| {
        let json = format!(
            r#"{{"type":"itrace","clk":{},"event":"complete","funct":"0x{:02x}","pc":"0x{:016x}","rs1":"0x{:016x}","rs2":"0x{:016x}"}}"#,
            trace.bemu_clk(),
            event.funct,
            event.pc,
            event.rs1,
            event.rs2
        );

        trace.write_itrace(&json);
    });
}
