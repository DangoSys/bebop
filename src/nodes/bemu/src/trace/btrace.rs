use super::trace::{bemu_clk, trace_state};
use bebop_bank_hash::{
    submit_runtime_bank_hash_packet, BankHashEventClass, BankHashPacket, BankHashPacketId, BankHashSource,
    BankHashTime, CanonicalBankHashPacket,
};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::Write;
use std::path::Path;

const RAW_LOG_FILE: &str = "bemu_bank_hash.ndjson";
const BTRACE_LOG_FILE: &str = "btrace_log.ndjson";

#[derive(Debug)]
pub(super) struct BtraceState {
    raw_line: u64,
    next_comparable_seq: u64,
    instruction_comparable_seq: BTreeMap<u64, u64>,
    bank_hash_file: Option<File>,
    btrace_log: Option<File>,
}

impl Default for BtraceState {
    fn default() -> Self {
        Self {
            raw_line: 0,
            next_comparable_seq: 1,
            instruction_comparable_seq: BTreeMap::new(),
            bank_hash_file: None,
            btrace_log: None,
        }
    }
}

impl BtraceState {
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

pub(super) fn init(log_dir: &Path, enabled: bool) -> io::Result<BtraceState> {
    if !enabled {
        return Ok(BtraceState::default());
    }

    let raw_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_dir.join(RAW_LOG_FILE))?;
    let btrace_log = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_dir.join(BTRACE_LOG_FILE))?;

    Ok(BtraceState {
        bank_hash_file: Some(raw_file),
        btrace_log: Some(btrace_log),
        ..BtraceState::default()
    })
}

pub(super) fn shutdown() -> io::Result<()> {
    let mut state = trace_state().lock().unwrap();
    if let Some(file) = state.btrace.bank_hash_file.as_mut() {
        file.flush()?;
    }
    if let Some(file) = state.btrace.btrace_log.as_mut() {
        file.flush()?;
    }
    state.btrace.bank_hash_file = None;
    state.btrace.btrace_log = None;
    Ok(())
}

fn write_bank_hash_trace(line: &str) {
    let mut state = trace_state().lock().unwrap();
    if let Some(file) = state.btrace.bank_hash_file.as_mut() {
        file.write_all(line.as_bytes()).ok();
        file.flush().ok();
    }
}

fn write_btrace_log(line: &str) {
    let mut state = trace_state().lock().unwrap();
    if let Some(file) = state.btrace.btrace_log.as_mut() {
        file.write_all(line.as_bytes()).ok();
        file.flush().ok();
    }
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
    let raw_line = trace_state().lock().unwrap().btrace.next_raw_line();
    if let Ok(line) = packet.to_ndjson() {
        write_bank_hash_trace(&line);
    }

    let event_class = classify_bemu_bank_hash(funct7);
    let comparable_seq = trace_state()
        .lock()
        .unwrap()
        .btrace
        .comparable_seq(instruction_id, event_class);
    let btrace_packet = CanonicalBankHashPacket::new(
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
    if let Ok(line) = btrace_packet.to_ndjson() {
        write_btrace_log(&line);
    }
    submit_runtime_bank_hash_packet(&btrace_packet);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparable_seq_reuses_instruction_id_when_events_are_not_contiguous() {
        let mut state = BtraceState::default();

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
