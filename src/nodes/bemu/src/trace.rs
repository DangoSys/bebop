// Trace logging for BEMU (NDJSON format)

use bebop_bank_hash::{
    init_runtime_packet_stream, shutdown_runtime_packet_stream, submit_runtime_bank_hash_packet, BankHashEventClass,
    BankHashPacket, BankHashPacketId, BankHashSource, BankHashTime, CanonicalBankHashPacket,
};
use std::collections::BTreeMap;
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
static BANK_HASH_TRACE_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static CANONICAL_BANK_HASH_TRACE_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static CANONICAL_STATE: OnceLock<Mutex<CanonicalState>> = OnceLock::new();

fn get_trace_file() -> &'static Mutex<Option<File>> {
    TRACE_FILE.get_or_init(|| Mutex::new(None))
}

fn get_bemu_clk() -> &'static Mutex<u64> {
    BEMU_CLK.get_or_init(|| Mutex::new(0))
}

fn get_bank_hash_trace_file() -> &'static Mutex<Option<File>> {
    BANK_HASH_TRACE_FILE.get_or_init(|| Mutex::new(None))
}

fn get_canonical_bank_hash_trace_file() -> &'static Mutex<Option<File>> {
    CANONICAL_BANK_HASH_TRACE_FILE.get_or_init(|| Mutex::new(None))
}

fn get_canonical_state() -> &'static Mutex<CanonicalState> {
    CANONICAL_STATE.get_or_init(|| Mutex::new(CanonicalState::new()))
}

#[derive(Debug)]
struct CanonicalState {
    raw_line: u64,
    next_comparable_seq: u64,
    instruction_comparable_seq: BTreeMap<u64, u64>,
}

impl CanonicalState {
    fn new() -> Self {
        Self {
            raw_line: 0,
            next_comparable_seq: 1,
            instruction_comparable_seq: BTreeMap::new(),
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn next_raw_line(&mut self) -> u64 {
        self.raw_line = self.raw_line.wrapping_add(1);
        self.raw_line
    }

    fn comparable_seq(&mut self, instruction_id: u64, event_class: BankHashEventClass) -> Option<u64> {
        if event_class != BankHashEventClass::BankDataWrite {
            return None;
        }

        if let Some(seq) = self.instruction_comparable_seq.get(&instruction_id) {
            return Some(*seq);
        }

        let seq = self.next_comparable_seq;
        self.next_comparable_seq = self.next_comparable_seq.wrapping_add(1);
        self.instruction_comparable_seq.insert(instruction_id, seq);
        Some(seq)
    }
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

pub fn init_bank_hash_trace(log_path: &Path, runtime_stream_path: Option<&Path>) -> io::Result<()> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_path)?;

    *get_bank_hash_trace_file().lock().unwrap() = Some(file);
    let canonical_path = log_path.with_file_name("bemu_bank_hash.canonical.ndjson");
    let canonical_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(canonical_path)?;
    *get_canonical_bank_hash_trace_file().lock().unwrap() = Some(canonical_file);
    get_canonical_state().lock().unwrap().reset();
    if let Some(path) = runtime_stream_path {
        init_runtime_packet_stream(path)?;
    }
    Ok(())
}

pub fn shutdown_bank_hash_trace() -> io::Result<()> {
    shutdown_runtime_packet_stream()
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

fn write_bank_hash_trace(line: &str) {
    if let Some(ref mut file) = *get_bank_hash_trace_file().lock().unwrap() {
        file.write_all(line.as_bytes()).ok();
        file.flush().ok();
    }
}

fn write_canonical_bank_hash_trace(line: &str) {
    if let Some(ref mut file) = *get_canonical_bank_hash_trace_file().lock().unwrap() {
        file.write_all(line.as_bytes()).ok();
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

    write_trace(&json);
}

fn classify_bemu_bank_hash(funct7: u32) -> BankHashEventClass {
    match funct7 {
        0 | 1 | 3 | 4 => BankHashEventClass::ControlOnly,
        2 | 32 | 34 | 80..=86 | 96..=104 => BankHashEventClass::ConfigOnly,
        16 | 35 | 87 | 105 => BankHashEventClass::MemoryOnly,
        33 | 48 | 49 | 50 | 51 | 52 | 53 | 55 | 64 | 65 | 66 | 67 => BankHashEventClass::BankDataWrite,
        other => {
            eprintln!("warning: unknown BEMU bank hash event class for funct7_{other}");
            BankHashEventClass::Unknown
        }
    }
}

pub fn bemu_bank_hash(instruction_id: u64, bank_id: u32, funct7: u32, op_type: &str, hash: u64, pc: u64) {
    let packet = BankHashPacket::new(
        BankHashSource::Bemu,
        BankHashPacketId::InstructionId(instruction_id),
        bank_id,
        op_type,
        hash,
        BankHashTime::Cycle(bemu_clk()),
    );
    let raw_line = get_canonical_state().lock().unwrap().next_raw_line();
    if let Ok(line) = packet.to_ndjson() {
        write_bank_hash_trace(&line);
    }

    let event_class = classify_bemu_bank_hash(funct7);
    let comparable_seq = get_canonical_state()
        .lock()
        .unwrap()
        .comparable_seq(instruction_id, event_class);
    let canonical = CanonicalBankHashPacket::new(
        BankHashSource::Bemu,
        instruction_id,
        comparable_seq,
        bank_id,
        funct7,
        op_type,
        event_class,
        hash,
        BankHashTime::Cycle(bemu_clk()),
        Some(pc),
        format!("bemu_bank_hash.ndjson:{raw_line}"),
        raw_line,
    );
    if let Ok(line) = canonical.to_ndjson() {
        write_canonical_bank_hash_trace(&line);
    }
    submit_runtime_bank_hash_packet(&canonical);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparable_seq_reuses_instruction_id_when_events_are_not_contiguous() {
        let mut state = CanonicalState::new();

        assert_eq!(state.comparable_seq(10, BankHashEventClass::BankDataWrite), Some(1));
        assert_eq!(state.comparable_seq(11, BankHashEventClass::BankDataWrite), Some(2));
        assert_eq!(state.comparable_seq(10, BankHashEventClass::BankDataWrite), Some(1));
        assert_eq!(state.comparable_seq(12, BankHashEventClass::ConfigOnly), None);
        assert_eq!(state.comparable_seq(12, BankHashEventClass::BankDataWrite), Some(3));
    }

    #[test]
    fn classifies_all_registered_npu_ops() {
        for funct7 in [
            0, 1, 2, 3, 4, 16, 32, 33, 34, 35, 48, 49, 50, 51, 52, 53, 55, 64, 65, 66, 67, 80, 81, 82, 83, 84, 85, 86,
            87, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105,
        ] {
            assert_ne!(classify_bemu_bank_hash(funct7), BankHashEventClass::Unknown);
        }

        assert_eq!(classify_bemu_bank_hash(0), BankHashEventClass::ControlOnly);
        assert_eq!(classify_bemu_bank_hash(32), BankHashEventClass::ConfigOnly);
        assert_eq!(classify_bemu_bank_hash(16), BankHashEventClass::MemoryOnly);
        assert_eq!(classify_bemu_bank_hash(53), BankHashEventClass::BankDataWrite);
    }
}
