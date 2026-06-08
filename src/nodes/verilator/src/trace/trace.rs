// Trace logging (NDJSON format)

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

static TRACE_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static ENABLE_ITRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static ENABLE_MTRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static ENABLE_PMCTRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static ENABLE_CTRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static ENABLE_BANKTRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static RTL_CLK: OnceLock<Mutex<u64>> = OnceLock::new();

fn get_trace_file() -> &'static Mutex<Option<File>> {
    TRACE_FILE.get_or_init(|| Mutex::new(None))
}

fn get_rtl_clk() -> &'static Mutex<u64> {
    RTL_CLK.get_or_init(|| Mutex::new(0))
}

#[derive(Clone, Debug, Default)]
pub struct TraceConfig {
    pub itrace: bool,
    pub mtrace: bool,
    pub pmctrace: bool,
    pub ctrace: bool,
    pub banktrace: bool,
}

pub struct ITraceEvent {
    pub is_issue: u8,
    pub rob_id: u32,
    pub domain_id: u32,
    pub funct: u32,
    pub pc: u64,
    pub rs1: u64,
    pub rs2: u64,
    pub bank_enable: u8,
}

pub struct MTraceEvent {
    pub is_write: u8,
    pub is_shared: u8,
    pub channel: u32,
    pub hart_id: u64,
    pub vbank_id: u32,
    pub pbank_id: u32,
    pub group_id: u32,
    pub addr: u32,
    pub data_lo: u64,
    pub data_hi: u64,
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
    *ENABLE_PMCTRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() = config.pmctrace;
    *ENABLE_CTRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() = config.ctrace;
    *ENABLE_BANKTRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() = config.banktrace;
    Ok(())
}

pub fn set_rtl_clk(clk: u64) {
    *get_rtl_clk().lock().unwrap() = clk;
}

pub fn rtl_clk() -> u64 {
    *get_rtl_clk().lock().unwrap()
}

fn write_trace(json: &str) {
    if let Some(ref mut file) = *get_trace_file().lock().unwrap() {
        writeln!(file, "{}", json).ok();
        file.flush().ok();
    }
}

// Instruction trace
pub fn itrace(event: ITraceEvent) {
    if !*ENABLE_ITRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() {
        return;
    }

    let bank_str = match event.bank_enable {
        0 => "---",
        1 => "R--",
        2 => "--W",
        3 => "R-W",
        4 => "RRW",
        _ => "---",
    };

    let clk = rtl_clk();
    let event_name = match event.is_issue {
        2 => "alloc",
        1 => "issue",
        _ => "complete",
    };

    let json = if event.is_issue >= 1 {
        format!(
            r#"{{"type":"itrace","clk":{},"event":"{}","rob_id":{},"domain_id":{},"funct":"0x{:02x}","bank_enable":{},"bank":"{}","pc":"0x{:016x}","rs1":"0x{:016x}","rs2":"0x{:016x}"}}"#,
            clk,
            event_name,
            event.rob_id,
            event.domain_id,
            event.funct,
            event.bank_enable,
            bank_str,
            event.pc,
            event.rs1,
            event.rs2
        )
    } else {
        format!(
            r#"{{"type":"itrace","clk":{},"event":"{}","rob_id":{},"domain_id":{},"funct":"0x{:02x}","bank_enable":{},"bank":"{}","pc":"0x{:016x}"}}"#,
            clk, event_name, event.rob_id, event.domain_id, event.funct, event.bank_enable, bank_str, event.pc
        )
    };

    write_trace(&json);
}

// Memory trace
pub fn mtrace(event: MTraceEvent) {
    if !*ENABLE_MTRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() {
        return;
    }

    let clk = rtl_clk();
    let json = if event.is_write != 0 {
        format!(
            r#"{{"type":"mtrace","clk":{},"event":"write","channel":{},"hart_id":{},"is_shared":{},"vbank_id":{},"pbank_id":{},"group_id":{},"addr":"0x{:08x}","data":"0x{:016x}{:016x}"}}"#,
            clk,
            event.channel,
            event.hart_id,
            event.is_shared,
            event.vbank_id,
            event.pbank_id,
            event.group_id,
            event.addr,
            event.data_hi,
            event.data_lo
        )
    } else {
        format!(
            r#"{{"type":"mtrace","clk":{},"event":"read","channel":{},"hart_id":{},"is_shared":{},"vbank_id":{},"pbank_id":{},"group_id":{},"addr":"0x{:08x}"}}"#,
            clk,
            event.channel,
            event.hart_id,
            event.is_shared,
            event.vbank_id,
            event.pbank_id,
            event.group_id,
            event.addr
        )
    };

    write_trace(&json);
}

// PMC trace (Ball)
pub fn pmctrace_ball(ball_id: u32, rob_id: u32, elapsed: u64) {
    if !*ENABLE_PMCTRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() {
        return;
    }

    let clk = rtl_clk();
    let json = format!(
        r#"{{"type":"pmctrace","clk":{},"event":"ball","ball_id":{},"rob_id":{},"elapsed":{}}}"#,
        clk, ball_id, rob_id, elapsed
    );

    write_trace(&json);
}

// PMC trace (Memory)
pub fn pmctrace_mem(is_store: u8, rob_id: u32, elapsed: u64) {
    if !*ENABLE_PMCTRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() {
        return;
    }

    let clk = rtl_clk();
    let event = if is_store != 0 { "store" } else { "load" };
    let json = format!(
        r#"{{"type":"pmctrace","clk":{},"event":"{}","rob_id":{},"elapsed":{}}}"#,
        clk, event, rob_id, elapsed
    );

    write_trace(&json);
}

// Cycle counter trace
pub fn ctrace(subcmd: u8, ctr_id: u32, tag: u64, elapsed: u64, cycle: u64) {
    if !*ENABLE_CTRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() {
        return;
    }

    let clk = rtl_clk();
    let json = match subcmd {
        0 => format!(
            r#"{{"type":"ctrace","clk":{},"event":"ctr_start","ctr_id":{},"tag":"0x{:X}","cycle":{}}}"#,
            clk, ctr_id, tag, cycle
        ),
        1 => format!(
            r#"{{"type":"ctrace","clk":{},"event":"ctr_stop","ctr_id":{},"tag":"0x{:X}","elapsed":{},"cycle":{}}}"#,
            clk, ctr_id, tag, elapsed, cycle
        ),
        2 => format!(
            r#"{{"type":"ctrace","clk":{},"event":"ctr_read","ctr_id":{},"current":{},"cycle":{}}}"#,
            clk, ctr_id, elapsed, cycle
        ),
        _ => return,
    };

    write_trace(&json);
}
