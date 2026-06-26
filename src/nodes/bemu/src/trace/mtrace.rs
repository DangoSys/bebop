use super::trace::{bytes_to_hex, with_current_trace};
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

const LOG_FILE: &str = "mtrace.ndjson";

pub struct MTraceEvent {
    pub is_write: bool,
    pub addr: u64,
    pub data: Vec<u8>,
    pub vbank_id: u32,
    pub pbank_id: u32,
    pub group_id: u32,
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

pub fn mtrace(event: MTraceEvent) {
    with_current_trace(|trace| {
        let data_hex = bytes_to_hex(&event.data);
        let event_name = if event.is_write { "write" } else { "read" };
        let json = format!(
            r#"{{"type":"mtrace","clk":{},"event":"{}","addr":"0x{:016x}","data":"0x{}","vbank_id":{},"pbank_id":{},"group_id":{}}}"#,
            trace.bemu_clk(),
            event_name,
            event.addr,
            data_hex,
            event.vbank_id,
            event.pbank_id,
            event.group_id
        );

        trace.write_mtrace(&json);
    });
}
