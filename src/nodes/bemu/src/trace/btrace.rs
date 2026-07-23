use super::trace::with_current_trace;
use bebop_bank_hash::{
    BankHashEventClass, BankHashPacket, BankHashPacketId, BankHashSource, BankHashTime, CanonicalBankHashPacket,
};
use std::fs::{File, OpenOptions};
use std::io;
use std::io::Write;
use std::path::Path;

const RAW_LOG_FILE: &str = "bemu_bank_hash.ndjson";
const BTRACE_LOG_FILE: &str = "btrace_log.ndjson";

#[derive(Debug)]
pub(super) struct BtraceState {
    raw_line: u64,
    bank_hash_file: Option<File>,
    btrace_log: Option<File>,
}

impl Default for BtraceState {
    fn default() -> Self {
        Self {
            raw_line: 0,
            bank_hash_file: None,
            btrace_log: None,
        }
    }
}

impl BtraceState {
    pub(super) fn enabled(&self) -> bool {
        self.bank_hash_file.is_some() || self.btrace_log.is_some()
    }

    fn next_raw_line(&mut self) -> u64 {
        self.raw_line = self.raw_line.wrapping_add(1);
        self.raw_line
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

fn write_bank_hash_trace(state: &mut BtraceState, line: &str) {
    if let Some(file) = state.bank_hash_file.as_mut() {
        file.write_all(line.as_bytes()).ok();
        file.flush().ok();
    }
}

fn write_btrace_log(state: &mut BtraceState, line: &str) {
    if let Some(file) = state.btrace_log.as_mut() {
        file.write_all(line.as_bytes()).ok();
        file.flush().ok();
    }
}

pub fn bemu_bank_hash(
    instruction_id: u64,
    semantic_seq: u64,
    bank_id: u32,
    version: u32,
    funct7: u32,
    op_type: &str,
    hash: u64,
    pc: u64,
) {
    with_current_trace(|trace| {
        let mut packet = BankHashPacket::new(
            BankHashSource::Bemu,
            BankHashPacketId::InstructionId(instruction_id),
            bank_id,
            op_type,
            hash,
            BankHashTime::Cycle(trace.bemu_clk()),
        );
        packet.version = version;
        let raw_line = trace.btrace.next_raw_line();
        if let Ok(line) = packet.to_ndjson() {
            write_bank_hash_trace(&mut trace.btrace, &line);
        }

        let btrace_packet = CanonicalBankHashPacket::new(
            BankHashSource::Bemu,
            instruction_id,
            Some(semantic_seq),
            bank_id,
            funct7,
            op_type,
            BankHashEventClass::BankDataWrite,
            hash,
            BankHashTime::Cycle(trace.bemu_clk()),
            Some(pc),
            format!("bemu_bank_hash.ndjson:{raw_line}"),
            raw_line,
        )
        .with_bank_version(version);
        if let Ok(line) = btrace_packet.to_ndjson() {
            write_btrace_log(&mut trace.btrace, &line);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_lines_are_monotonic() {
        let mut state = BtraceState::default();
        assert_eq!(state.next_raw_line(), 1);
        assert_eq!(state.next_raw_line(), 2);
    }
}
