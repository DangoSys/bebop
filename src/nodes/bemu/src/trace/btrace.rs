use super::trace::with_current_trace;
use bebop_bank_hash::{
    submit_runtime_bank_digest, BankDigestRecord, BankHashEventClass, BankHashSource, BankHashTime, LogicalBankId,
};
use std::fs::{File, OpenOptions};
use std::io;
use std::io::Write;
use std::path::Path;

const GOLDEN_RECORD_FILE: &str = "bemu_bank_digest.ndjson";

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
pub fn bemu_bank_digest(
    instruction_id: u64,
    vbank_id: u32,
    group_id: u32,
    physical_bank_id: u32,
    funct7: u32,
    op_type: &str,
    digest: u64,
    pc: u64,
) {
    with_current_trace(|trace| {
        let line_number = trace.btrace.next_line();
        let record = BankDigestRecord::new(
            BankHashSource::Bemu,
            instruction_id,
            LogicalBankId::new(vbank_id, group_id),
            Some(physical_bank_id),
            digest,
            funct7,
            op_type,
            BankHashEventClass::BankDataWrite,
            BankHashTime::Cycle(trace.bemu_clk()),
            Some(pc),
            Some(format!("{GOLDEN_RECORD_FILE}:{line_number}")),
        );

        if let (Some(file), Ok(line)) = (trace.btrace.golden_record_file.as_mut(), record.to_ndjson()) {
            file.write_all(line.as_bytes()).ok();
            file.flush().ok();
        }
        submit_runtime_bank_digest(&record);
    });
}
