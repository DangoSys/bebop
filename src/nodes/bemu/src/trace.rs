// Trace logging for BEMU (NDJSON format)

use std::fs::{File, OpenOptions};
use std::io;
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

static TRACE_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static ENABLE_ITRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static ENABLE_MTRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static ENABLE_BANKTRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static BEMU_CLK: OnceLock<Mutex<u64>> = OnceLock::new();

fn get_trace_file() -> &'static Mutex<Option<File>> {
    TRACE_FILE.get_or_init(|| Mutex::new(None))
}

fn get_bemu_clk() -> &'static Mutex<u64> {
    BEMU_CLK.get_or_init(|| Mutex::new(0))
}

#[derive(Clone, Debug, Default)]
pub struct TraceConfig {
    pub itrace: bool,
    pub mtrace: bool,
    pub banktrace: bool,
}

pub struct ITraceEvent {
    pub funct: u32,
    pub pc: u64,
    pub rs1: u64,
    pub rs2: u64,
}

pub struct MTraceEvent {
    pub is_write: bool,
    pub addr: u64,
    pub data: Vec<u8>,
    pub vbank_id: u32,
    pub pbank_id: u32,
    pub group_id: u32,
}

pub struct BankTraceEvent {
    pub event: String,
    pub vbank_id: u32,
    pub pbank_id: u32,
    pub group_id: u32,
    pub data: Option<Vec<u8>>,
    pub addr: Option<u64>,
}

pub fn init_trace(log_path: &Path, config: TraceConfig) -> io::Result<()> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_path)?;

    *get_trace_file().lock().unwrap() = Some(file);
    *ENABLE_ITRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() = config.itrace;
    *ENABLE_MTRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() = config.mtrace;
    *ENABLE_BANKTRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() = config.banktrace;
    Ok(())
}

pub fn set_bemu_clk(clk: u64) {
    *get_bemu_clk().lock().unwrap() = clk;
}

pub fn bemu_clk() -> u64 {
    *get_bemu_clk().lock().unwrap()
}

fn write_trace(json: &str) {
    if let Some(ref mut file) = *get_trace_file().lock().unwrap() {
        writeln!(file, "{}", json).ok();
        file.flush().ok();
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().rev().map(|b| format!("{:02x}", b)).collect()
}

// Instruction trace
pub fn itrace(event: ITraceEvent) {
    if !*ENABLE_ITRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() {
        return;
    }

    let clk = bemu_clk();
    let json = format!(
        r#"{{"type":"itrace","clk":{},"event":"complete","funct":"0x{:02x}","pc":"0x{:016x}","rs1":"0x{:016x}","rs2":"0x{:016x}"}}"#,
        clk, event.funct, event.pc, event.rs1, event.rs2
    );

    write_trace(&json);
}

// Memory trace
pub fn mtrace(event: MTraceEvent) {
    if !*ENABLE_MTRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() {
        return;
    }

    let clk = bemu_clk();
    let data_hex = bytes_to_hex(&event.data);
    let event_name = if event.is_write { "write" } else { "read" };
    let json = format!(
        r#"{{"type":"mtrace","clk":{},"event":"{}","addr":"0x{:016x}","data":"0x{}","vbank_id":{},"pbank_id":{},"group_id":{}}}"#,
        clk, event_name, event.addr, data_hex, event.vbank_id, event.pbank_id, event.group_id
    );

    write_trace(&json);
}

// Bank trace
pub fn banktrace(event: BankTraceEvent) {
    if !*ENABLE_BANKTRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() {
        return;
    }

    let clk = bemu_clk();
    let data_hex = event.data.as_ref().map(|d| bytes_to_hex(d)).unwrap_or_default();
    let addr_str = event.addr.map(|a| format!("\"0x{:016x}\"", a)).unwrap_or_else(|| "null".to_string());
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

    write_trace(&json);
}
