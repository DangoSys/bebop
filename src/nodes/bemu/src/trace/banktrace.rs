use super::trace::{bemu_clk, bytes_to_hex, write_banktrace};
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

const LOG_FILE: &str = "banktrace.ndjson";

pub struct BankTraceEvent {
    pub event: String,
    pub vbank_id: u32,
    pub pbank_id: u32,
    pub group_id: u32,
    pub data: Option<Vec<u8>>,
    pub addr: Option<u64>,
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

pub fn banktrace(event: BankTraceEvent) {
    let clk = bemu_clk();
    let data_hex = event.data.as_ref().map(|d| bytes_to_hex(d)).unwrap_or_default();
    let addr_str = event
        .addr
        .map(|a| format!("\"0x{:016x}\"", a))
        .unwrap_or_else(|| "null".to_string());
    let json = if event.data.is_some() {
        format!(
            r#"{{"type":"banktrace","clk":{},"event":"{}","vbank_id":{},"pbank_id":{},"group_id":{},"addr":{},"data":"0x{}"}}"#,
            clk, event.event, event.vbank_id, event.pbank_id, event.group_id, addr_str, data_hex
        )
    } else {
        format!(
            r#"{{"type":"banktrace","clk":{},"event":"{}","vbank_id":{},"pbank_id":{},"group_id":{},"addr":{}}}"#,
            clk, event.event, event.vbank_id, event.pbank_id, event.group_id, addr_str
        )
    };

    write_banktrace(&json);
}
