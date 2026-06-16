// Trace logging (NDJSON format)

use crate::ffi::{
    verilator_private_bank_pending_writes, verilator_read_bank_scoreboard, verilator_read_private_bank, VerilatorTop,
};
use bebop_bank_hash::{
    bank_hash, init_runtime_packet_stream, shutdown_runtime_packet_stream, submit_runtime_bank_hash_packet,
    BankHashEventClass, BankHashPacket, BankHashPacketId, BankHashSource, BankHashTime, CanonicalBankHashPacket,
    BANK_NUM, BANK_SIZE,
};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Mutex, OnceLock};

static TRACE_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static ENABLE_ITRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static ENABLE_MTRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static ENABLE_PMCTRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static ENABLE_CTRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static ENABLE_BANKTRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static RTL_CLK: OnceLock<Mutex<u64>> = OnceLock::new();
static RTL_BANK_HASH_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static RTL_CANONICAL_BANK_HASH_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static RTL_BANK_STABILITY_MONITOR: OnceLock<Mutex<BankStabilityMonitor>> = OnceLock::new();
static RTL_CANONICAL_STATE: OnceLock<Mutex<CanonicalState>> = OnceLock::new();
static VERILATOR_TOP: OnceLock<AtomicPtr<VerilatorTop>> = OnceLock::new();

fn get_trace_file() -> &'static Mutex<Option<File>> {
    TRACE_FILE.get_or_init(|| Mutex::new(None))
}

fn get_rtl_clk() -> &'static Mutex<u64> {
    RTL_CLK.get_or_init(|| Mutex::new(0))
}

fn get_rtl_bank_hash_file() -> &'static Mutex<Option<File>> {
    RTL_BANK_HASH_FILE.get_or_init(|| Mutex::new(None))
}

fn get_rtl_canonical_bank_hash_file() -> &'static Mutex<Option<File>> {
    RTL_CANONICAL_BANK_HASH_FILE.get_or_init(|| Mutex::new(None))
}

fn get_rtl_bank_stability_monitor() -> &'static Mutex<BankStabilityMonitor> {
    RTL_BANK_STABILITY_MONITOR.get_or_init(|| Mutex::new(BankStabilityMonitor::new()))
}

fn get_rtl_canonical_state() -> &'static Mutex<CanonicalState> {
    RTL_CANONICAL_STATE.get_or_init(|| Mutex::new(CanonicalState::new()))
}

#[derive(Debug)]
struct CanonicalState {
    raw_line: u64,
    next_comparable_seq: u64,
}

impl CanonicalState {
    fn new() -> Self {
        Self {
            raw_line: 0,
            next_comparable_seq: 1,
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn next_raw_line(&mut self) -> u64 {
        self.raw_line = self.raw_line.wrapping_add(1);
        self.raw_line
    }

    fn allocate_comparable_seq(&mut self, event_class: BankHashEventClass) -> Option<u64> {
        if event_class != BankHashEventClass::BankDataWrite {
            return None;
        }

        let seq = self.next_comparable_seq;
        self.next_comparable_seq = self.next_comparable_seq.wrapping_add(1);
        Some(seq)
    }
}

#[derive(Clone, Debug)]
struct StableHashTask {
    instruction_id: u64,
    comparable_seq: Option<u64>,
    bank_id: u32,
    funct7: u32,
    op_type: String,
    cycle: u64,
    pc: u64,
}

#[derive(Clone, Debug)]
struct PendingHashTask {
    task_id: u64,
    instruction_id: u64,
    comparable_seq: Option<u64>,
    bank_id: usize,
    funct7: u32,
    op_type: String,
    cycle: u64,
    pc: u64,
    observed_write_count: u64,
}

#[derive(Clone, Copy, Debug)]
struct BankStabilitySnapshot {
    pending_same_bank_writes: u32,
    scoreboard_rd_count: u32,
    scoreboard_wr_busy: bool,
}

#[derive(Debug)]
struct BankStabilityMonitor {
    write_request_banks: BTreeSet<usize>,
    write_request_counts: [u64; BANK_NUM],
    pending_hash_tasks: Vec<PendingHashTask>,
    task_count: u64,
    write_request_count: u64,
}

impl BankStabilityMonitor {
    fn new() -> Self {
        Self {
            write_request_banks: BTreeSet::new(),
            write_request_counts: [0; BANK_NUM],
            pending_hash_tasks: Vec::new(),
            task_count: 0,
            write_request_count: 0,
        }
    }

    fn reset(&mut self) {
        self.write_request_banks.clear();
        self.write_request_counts = [0; BANK_NUM];
        self.pending_hash_tasks.clear();
        self.task_count = 0;
        self.write_request_count = 0;
    }

    fn record_write_request(&mut self, pbank_id: u32) {
        let bank_id = pbank_id as usize;
        if bank_id >= BANK_NUM {
            return;
        }

        self.write_request_banks.insert(bank_id);
        self.write_request_counts[bank_id] = self.write_request_counts[bank_id].wrapping_add(1);
        self.write_request_count = self.write_request_count.wrapping_add(1);
    }

    fn complete_instruction(
        &mut self,
        instruction_id: u64,
        funct7: u32,
        op_type: &str,
        cycle: u64,
        pc: u64,
    ) -> Vec<StableHashTask> {
        if self.write_request_banks.is_empty() {
            return Vec::new();
        }

        let event_class = classify_rtl_bank_hash(funct7, pc);
        let comparable_seq = get_rtl_canonical_state()
            .lock()
            .unwrap()
            .allocate_comparable_seq(event_class);

        for bank_id in std::mem::take(&mut self.write_request_banks) {
            self.task_count = self.task_count.wrapping_add(1);
            let task = PendingHashTask {
                task_id: self.task_count,
                instruction_id,
                comparable_seq,
                bank_id,
                funct7,
                op_type: op_type.to_string(),
                cycle,
                pc,
                observed_write_count: self.write_request_counts[bank_id],
            };
            let snapshot = read_bank_stability_snapshot(bank_id as u32);

            write_bank_hash_stability_event("pending", &task, snapshot);
            self.pending_hash_tasks.push(task);
        }

        Vec::new()
    }

    fn poll_stable_tasks(&mut self) -> Vec<StableHashTask> {
        let mut stable_tasks = Vec::new();
        let mut still_pending = Vec::new();
        for task in std::mem::take(&mut self.pending_hash_tasks) {
            let snapshot = read_bank_stability_snapshot(task.bank_id as u32);

            if self.is_stable(&task, snapshot) {
                write_bank_hash_stability_event("stable", &task, snapshot);
                stable_tasks.push(StableHashTask {
                    instruction_id: task.instruction_id,
                    comparable_seq: task.comparable_seq,
                    bank_id: task.bank_id as u32,
                    funct7: task.funct7,
                    op_type: task.op_type.clone(),
                    cycle: task.cycle,
                    pc: task.pc,
                });
            } else {
                still_pending.push(task);
            }
        }
        self.pending_hash_tasks = still_pending;

        stable_tasks
    }

    fn is_stable(&self, task: &PendingHashTask, snapshot: BankStabilitySnapshot) -> bool {
        task.observed_write_count > 0
            && snapshot.pending_same_bank_writes == 0
            && snapshot.scoreboard_rd_count == 0
            && !snapshot.scoreboard_wr_busy
    }
}

fn get_verilator_top() -> &'static AtomicPtr<VerilatorTop> {
    VERILATOR_TOP.get_or_init(|| AtomicPtr::new(ptr::null_mut()))
}

pub fn set_verilator_top(top: *mut VerilatorTop) {
    get_verilator_top().store(top, Ordering::SeqCst);
}

fn read_rtl_private_bank_hash(bank_id: u32) -> Option<u64> {
    let top = get_verilator_top().load(Ordering::SeqCst);
    if top.is_null() {
        return None;
    }

    let mut bytes = vec![0u8; BANK_SIZE];
    let ok = unsafe { verilator_read_private_bank(top, bank_id, bytes.as_mut_ptr(), bytes.len() as u32) };
    ok.then(|| bank_hash(&bytes))
}

fn read_bank_stability_snapshot(bank_id: u32) -> BankStabilitySnapshot {
    let top = get_verilator_top().load(Ordering::SeqCst);
    if top.is_null() {
        return BankStabilitySnapshot {
            pending_same_bank_writes: u32::MAX,
            scoreboard_rd_count: u32::MAX,
            scoreboard_wr_busy: true,
        };
    }

    let pending_same_bank_writes = unsafe { verilator_private_bank_pending_writes(top, bank_id) };
    let mut scoreboard_rd_count = u32::MAX;
    let mut scoreboard_wr_busy = true;
    let ok = unsafe {
        verilator_read_bank_scoreboard(
            top,
            bank_id,
            &mut scoreboard_rd_count as *mut u32,
            &mut scoreboard_wr_busy as *mut bool,
        )
    };
    if !ok {
        scoreboard_rd_count = u32::MAX;
        scoreboard_wr_busy = true;
    }

    BankStabilitySnapshot {
        pending_same_bank_writes,
        scoreboard_rd_count,
        scoreboard_wr_busy,
    }
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

pub struct BankTraceEvent {
    pub event: &'static str,
    pub is_shared: u8,
    pub vbank_id: u32,
    pub pbank_id: u32,
    pub group_id: u32,
    pub addr: u32,
    pub data_lo: Option<u64>,
    pub data_hi: Option<u64>,
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

pub fn init_rtl_bank_hash_trace(log_path: &Path, runtime_stream_path: Option<&Path>) -> io::Result<()> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_path)?;

    *get_rtl_bank_hash_file().lock().unwrap() = Some(file);
    let canonical_path = log_path.with_file_name("rtl_bank_hash.canonical.ndjson");
    let canonical_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(canonical_path)?;
    *get_rtl_canonical_bank_hash_file().lock().unwrap() = Some(canonical_file);
    get_rtl_bank_stability_monitor().lock().unwrap().reset();
    get_rtl_canonical_state().lock().unwrap().reset();
    if let Some(path) = runtime_stream_path {
        init_runtime_packet_stream(path)?;
    }
    Ok(())
}

pub fn shutdown_rtl_bank_hash_trace() -> io::Result<()> {
    shutdown_runtime_packet_stream()
}

pub fn set_rtl_clk(clk: u64) {
    *get_rtl_clk().lock().unwrap() = clk;
}

pub fn rtl_clk() -> u64 {
    *get_rtl_clk().lock().unwrap()
}

pub fn poll_rtl_bank_hash_stability() {
    let stable_tasks = get_rtl_bank_stability_monitor().lock().unwrap().poll_stable_tasks();
    emit_stable_rtl_bank_hash_tasks(stable_tasks);
}

fn write_trace(json: &str) {
    if let Some(ref mut file) = *get_trace_file().lock().unwrap() {
        writeln!(file, "{}", json).ok();
        file.flush().ok();
    }
}

fn write_bank_hash_stability_event(event: &str, task: &PendingHashTask, snapshot: BankStabilitySnapshot) {
    let json = format!(
        r#"{{"type":"bank_hash_stability","clk":{},"event":"{}","task_id":{},"source":"RTL","instruction_id":{},"bank_id":{},"version":0,"funct7":{},"op_type":"{}","pc":"0x{:016x}","observed_write_count":{},"pending_same_bank_writes":{},"scoreboard_rd_count":{},"scoreboard_wr_busy":{},"strategy":"verilated_write_ack_and_bank_scoreboard"}}"#,
        task.cycle,
        event,
        task.task_id,
        task.instruction_id,
        task.bank_id,
        task.funct7,
        task.op_type,
        task.pc,
        task.observed_write_count,
        snapshot.pending_same_bank_writes,
        snapshot.scoreboard_rd_count,
        snapshot.scoreboard_wr_busy
    );
    write_trace(&json);
}

fn write_canonical_rtl_bank_hash_packet(line: &str) {
    if let Some(ref mut file) = *get_rtl_canonical_bank_hash_file().lock().unwrap() {
        file.write_all(line.as_bytes()).ok();
        file.flush().ok();
    }
}

fn classify_rtl_bank_hash(funct7: u32, pc: u64) -> BankHashEventClass {
    if pc == 0 {
        return BankHashEventClass::BootInit;
    }

    match funct7 {
        0 | 1 | 3 | 4 => BankHashEventClass::ControlOnly,
        2 | 32 | 34 | 80..=86 | 96..=104 => BankHashEventClass::ConfigOnly,
        16 | 35 | 87 | 105 => BankHashEventClass::MemoryOnly,
        33 | 48 | 49 | 50 | 51 | 52 | 53 | 55 | 64 | 65 | 66 | 67 => BankHashEventClass::BankDataWrite,
        other => {
            eprintln!("warning: unknown RTL bank hash event class for funct7_{other} pc=0x{pc:016x}");
            BankHashEventClass::Unknown
        }
    }
}

fn write_rtl_bank_hash_packet(
    instruction_id: u64,
    comparable_seq: Option<u64>,
    bank_id: u32,
    funct7: u32,
    op_type: &str,
    hash: u64,
    cycle: u64,
    pc: u64,
) {
    let packet = BankHashPacket::new(
        BankHashSource::Rtl,
        BankHashPacketId::InstructionId(instruction_id),
        bank_id,
        op_type,
        hash,
        BankHashTime::Cycle(cycle),
    );
    let raw_line = get_rtl_canonical_state().lock().unwrap().next_raw_line();
    if let Ok(line) = packet.to_ndjson() {
        if let Some(ref mut file) = *get_rtl_bank_hash_file().lock().unwrap() {
            file.write_all(line.as_bytes()).ok();
            file.flush().ok();
        }
    }

    let event_class = classify_rtl_bank_hash(funct7, pc);
    let canonical = CanonicalBankHashPacket::new(
        BankHashSource::Rtl,
        instruction_id,
        comparable_seq,
        bank_id,
        funct7,
        op_type,
        event_class,
        hash,
        BankHashTime::Cycle(cycle),
        Some(pc),
        format!("rtl_bank_hash.ndjson:{raw_line}"),
        raw_line,
    );
    if let Ok(line) = canonical.to_ndjson() {
        write_canonical_rtl_bank_hash_packet(&line);
    }
    submit_runtime_bank_hash_packet(&canonical);
}

fn emit_stable_rtl_bank_hash_tasks(stable_tasks: Vec<StableHashTask>) {
    for task in stable_tasks {
        if let Some(hash) = read_rtl_private_bank_hash(task.bank_id) {
            write_rtl_bank_hash_packet(
                task.instruction_id,
                task.comparable_seq,
                task.bank_id,
                task.funct7,
                &task.op_type,
                hash,
                task.cycle,
                task.pc,
            );
        }
    }
}

// Instruction trace
pub fn itrace(event: ITraceEvent) {
    if event.is_issue == 0 {
        let op_type = format!("funct7_{}", event.funct);
        let stable_tasks = get_rtl_bank_stability_monitor().lock().unwrap().complete_instruction(
            event.rob_id as u64,
            event.funct,
            &op_type,
            rtl_clk(),
            event.pc,
        );
        emit_stable_rtl_bank_hash_tasks(stable_tasks);
    }

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
    if event.is_write != 0 {
        banktrace(BankTraceEvent {
            event: "backdoor_write",
            is_shared: event.is_shared,
            vbank_id: event.vbank_id,
            pbank_id: event.pbank_id,
            group_id: event.group_id,
            addr: event.addr,
            data_lo: Some(event.data_lo),
            data_hi: Some(event.data_hi),
        });
    } else {
        banktrace(BankTraceEvent {
            event: "backdoor_read",
            is_shared: event.is_shared,
            vbank_id: event.vbank_id,
            pbank_id: event.pbank_id,
            group_id: event.group_id,
            addr: event.addr,
            data_lo: None,
            data_hi: None,
        });
    }

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

pub fn banktrace(event: BankTraceEvent) {
    if event.event == "backdoor_write" && event.is_shared == 0 {
        get_rtl_bank_stability_monitor()
            .lock()
            .unwrap()
            .record_write_request(event.pbank_id);
    }

    if !*ENABLE_BANKTRACE.get_or_init(|| Mutex::new(false)).lock().unwrap() {
        return;
    }

    let clk = rtl_clk();
    let json = match (event.data_lo, event.data_hi) {
        (Some(data_lo), Some(data_hi)) => format!(
            r#"{{"type":"banktrace","clk":{},"event":"{}","bank_id":{},"row":{},"is_shared":{},"vbank_id":{},"pbank_id":{},"group_id":{},"addr":"0x{:08x}","data":"0x{:016x}{:016x}"}}"#,
            clk,
            event.event,
            event.pbank_id,
            event.addr,
            event.is_shared,
            event.vbank_id,
            event.pbank_id,
            event.group_id,
            event.addr,
            data_hi,
            data_lo
        ),
        _ => format!(
            r#"{{"type":"banktrace","clk":{},"event":"{}","bank_id":{},"row":{},"is_shared":{},"vbank_id":{},"pbank_id":{},"group_id":{},"addr":"0x{:08x}"}}"#,
            clk,
            event.event,
            event.pbank_id,
            event.addr,
            event.is_shared,
            event.vbank_id,
            event.pbank_id,
            event.group_id,
            event.addr
        ),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparable_seq_is_unique_per_rtl_bank_data_write_event() {
        let mut state = CanonicalState::new();

        assert_eq!(
            state.allocate_comparable_seq(BankHashEventClass::BankDataWrite),
            Some(1)
        );
        assert_eq!(
            state.allocate_comparable_seq(BankHashEventClass::BankDataWrite),
            Some(2)
        );
        assert_eq!(state.allocate_comparable_seq(BankHashEventClass::ConfigOnly), None);
        assert_eq!(
            state.allocate_comparable_seq(BankHashEventClass::BankDataWrite),
            Some(3)
        );
    }

    #[test]
    fn classifies_all_registered_npu_ops() {
        for funct7 in [
            0, 1, 2, 3, 4, 16, 32, 33, 34, 35, 48, 49, 50, 51, 52, 53, 55, 64, 65, 66, 67, 80, 81, 82, 83, 84, 85, 86,
            87, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105,
        ] {
            assert_ne!(classify_rtl_bank_hash(funct7, 0x8000_0000), BankHashEventClass::Unknown);
        }

        assert_eq!(classify_rtl_bank_hash(33, 0), BankHashEventClass::BootInit);
        assert_eq!(classify_rtl_bank_hash(0, 0x8000_0000), BankHashEventClass::ControlOnly);
        assert_eq!(classify_rtl_bank_hash(32, 0x8000_0000), BankHashEventClass::ConfigOnly);
        assert_eq!(classify_rtl_bank_hash(16, 0x8000_0000), BankHashEventClass::MemoryOnly);
        assert_eq!(
            classify_rtl_bank_hash(53, 0x8000_0000),
            BankHashEventClass::BankDataWrite
        );
    }
}
