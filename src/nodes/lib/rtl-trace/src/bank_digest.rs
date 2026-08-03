use crate::itrace::ITraceEvent;
use crate::mtrace::MTraceEvent;
use crate::state;
use bebop_bank_hash::{
    bank_hash, submit_runtime_bank_digest, BankDigestRecord, BankHashEventClass, BankHashSource, BankHashTime,
    LogicalBankId,
};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

const RTL_RECORD_FILE: &str = "rtl_bank_digest.ndjson";
const M2_QUIET_POLLS: u64 = 2;

/// Geometry required to reconstruct a complete private bank from actual RTL
/// write requests. M2 currently supports the 128-bit private-bank trace port.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BankDigestConfig {
    pub bank_size: usize,
    pub row_bytes: usize,
}

impl BankDigestConfig {
    pub fn new(bank_size: usize, row_bytes: usize) -> Self {
        Self { bank_size, row_bytes }
    }
}

#[derive(Clone, Debug)]
struct InstructionMeta {
    instruction_id: u64,
    funct7: u32,
    pc: u64,
    may_write_bank: bool,
}

#[derive(Clone, Debug)]
struct ActiveWriter {
    rob_id: u32,
    meta: InstructionMeta,
    bank_id: Option<LogicalBankId>,
    physical_bank_id: Option<u32>,
    wrote: bool,
    completed: bool,
    last_activity_poll: u64,
}

struct M2Monitor {
    config: BankDigestConfig,
    banks: BTreeMap<LogicalBankId, Vec<u8>>,
    instructions: BTreeMap<u32, InstructionMeta>,
    active_writer: Option<ActiveWriter>,
    next_instruction_id: u64,
    poll_count: u64,
    next_line: u64,
    output: File,
    error: Option<String>,
}

impl M2Monitor {
    fn new(log_dir: &Path, config: BankDigestConfig) -> io::Result<Self> {
        if config.bank_size == 0 || config.row_bytes != 16 || config.bank_size % config.row_bytes != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "M2 requires a non-zero bank size divisible by the 16-byte RTL trace row: bank_size={} row_bytes={}",
                    config.bank_size, config.row_bytes
                ),
            ));
        }
        let output = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(log_dir.join(RTL_RECORD_FILE))?;
        Ok(Self {
            config,
            banks: BTreeMap::new(),
            instructions: BTreeMap::new(),
            active_writer: None,
            next_instruction_id: 0,
            poll_count: 0,
            next_line: 0,
            output,
            error: None,
        })
    }

    fn fail(&mut self, message: impl Into<String>) {
        if self.error.is_none() {
            self.error = Some(format!("M2 unsupported: {}", message.into()));
        }
    }

    fn record_instruction(&mut self, event: &ITraceEvent) {
        // Reset-time trace traffic is not part of the architectural command stream.
        if event.pc == 0 {
            return;
        }

        match event.is_issue {
            2 => self.allocate(event),
            0 => self.complete(event.rob_id),
            _ => {}
        }
    }

    fn allocate(&mut self, event: &ITraceEvent) {
        self.next_instruction_id = self.next_instruction_id.wrapping_add(1);
        let meta = InstructionMeta {
            instruction_id: self.next_instruction_id,
            funct7: event.funct,
            pc: event.pc,
            may_write_bank: matches!(event.bank_enable, 2 | 3 | 4),
        };

        // mset is configuration, not a data-write target inference. Clearing
        // its logical shadow mirrors BEMU allocation initialization.
        if event.funct == 32 {
            let vbank_id = (event.rs1 & 0x3ff) as u32;
            let alloc = ((event.rs2 >> 10) & 1) != 0;
            self.banks.retain(|id, _| id.vbank_id != vbank_id);
            if alloc {
                let groups = ((event.rs2 >> 5) & 0x1f).max(1) as u32;
                for group_id in 0..groups {
                    self.banks
                        .insert(LogicalBankId::new(vbank_id, group_id), vec![0; self.config.bank_size]);
                }
            }
        }

        if self.instructions.insert(event.rob_id, meta.clone()).is_some() {
            self.fail(format!("duplicate RTL ROB allocation {}", event.rob_id));
            return;
        }
        if meta.may_write_bank {
            if let Some(active) = &self.active_writer {
                self.fail(format!(
                    "overlapping bank writers: instruction {} (ROB {}) and instruction {} (ROB {})",
                    active.meta.instruction_id, active.rob_id, meta.instruction_id, event.rob_id
                ));
                return;
            }
            self.active_writer = Some(ActiveWriter {
                rob_id: event.rob_id,
                meta,
                bank_id: None,
                physical_bank_id: None,
                wrote: false,
                completed: false,
                last_activity_poll: self.poll_count,
            });
        }
    }

    fn complete(&mut self, rob_id: u32) {
        let Some(meta) = self.instructions.remove(&rob_id) else {
            self.fail(format!("completion without allocation for ROB {rob_id}"));
            return;
        };
        if meta.may_write_bank {
            let Some(active) = self.active_writer.as_mut() else {
                self.fail(format!(
                    "writer instruction {} completed without an active writer",
                    meta.instruction_id
                ));
                return;
            };
            if active.rob_id != rob_id {
                if self.error.is_none() {
                    self.error = Some(format!(
                        "M2 unsupported: writer completion ROB {rob_id} does not match active writer ROB {}",
                        active.rob_id
                    ));
                }
                return;
            }
            active.completed = true;
            active.last_activity_poll = self.poll_count;
        }
    }

    fn record_memory(&mut self, event: &MTraceEvent) {
        if event.is_write == 0 || event.is_shared != 0 {
            return;
        }
        let bank_id = LogicalBankId::new(event.vbank_id, event.group_id);
        let Some(active) = self.active_writer.as_mut() else {
            self.fail(format!(
                "private-bank write ({},{}) has no unique in-flight writer",
                bank_id.vbank_id, bank_id.group_id
            ));
            return;
        };
        if active.completed {
            // A DPI request can be delayed relative to itrace completion; it is
            // still attributed to the sole M2 writer until its quiet boundary.
            active.last_activity_poll = self.poll_count;
        }
        if let Some(previous) = active.bank_id {
            if previous != bank_id {
                if self.error.is_none() {
                    self.error = Some(format!(
                        "M2 unsupported: instruction {} wrote multiple logical banks: ({},{}) and ({},{})",
                        active.meta.instruction_id,
                        previous.vbank_id,
                        previous.group_id,
                        bank_id.vbank_id,
                        bank_id.group_id
                    ));
                }
                return;
            }
        } else {
            active.bank_id = Some(bank_id);
            active.physical_bank_id = Some(event.pbank_id);
        }
        if active.physical_bank_id != Some(event.pbank_id) {
            if self.error.is_none() {
                self.error = Some(format!(
                    "M2 unsupported: logical bank ({},{}) changed physical bank within instruction {}",
                    bank_id.vbank_id, bank_id.group_id, active.meta.instruction_id
                ));
            }
            return;
        }

        let Some(offset) = (event.addr as usize).checked_mul(self.config.row_bytes) else {
            self.fail(format!("bank row address overflow: {}", event.addr));
            return;
        };
        if offset + self.config.row_bytes > self.config.bank_size {
            self.fail(format!(
                "bank row {} exceeds bank size {}",
                event.addr, self.config.bank_size
            ));
            return;
        }
        let bank = self
            .banks
            .entry(bank_id)
            .or_insert_with(|| vec![0; self.config.bank_size]);
        bank[offset..offset + 8].copy_from_slice(&event.data_lo.to_le_bytes());
        bank[offset + 8..offset + 16].copy_from_slice(&event.data_hi.to_le_bytes());
        active.wrote = true;
        active.last_activity_poll = self.poll_count;
    }

    fn poll(&mut self, force: bool) -> Result<(), String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        self.poll_count = self.poll_count.wrapping_add(1);
        let ready = self.active_writer.as_ref().is_some_and(|active| {
            active.completed && (force || self.poll_count.saturating_sub(active.last_activity_poll) >= M2_QUIET_POLLS)
        });
        if !ready {
            return Ok(());
        }

        let active = self.active_writer.take().expect("ready writer exists");
        if !active.wrote {
            return Ok(());
        }
        let bank_id = active.bank_id.expect("a writer with writes has a bank id");
        let bytes = self.banks.get(&bank_id).expect("written shadow bank exists");
        self.next_line = self.next_line.wrapping_add(1);
        let record = BankDigestRecord::new(
            BankHashSource::Rtl,
            active.meta.instruction_id,
            bank_id,
            active.physical_bank_id,
            bank_hash(bytes),
            active.meta.funct7,
            format!("funct7_{}", active.meta.funct7),
            BankHashEventClass::BankDataWrite,
            BankHashTime::Cycle(state::rtl_clk()),
            Some(active.meta.pc),
            Some(format!("{RTL_RECORD_FILE}:{}", self.next_line)),
        );
        let line = record.to_ndjson().map_err(|error| error.to_string())?;
        self.output
            .write_all(line.as_bytes())
            .map_err(|error| error.to_string())?;
        self.output.flush().map_err(|error| error.to_string())?;
        submit_runtime_bank_digest(&record);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), String> {
        self.poll(true)?;
        if let Some(active) = &self.active_writer {
            return Err(format!(
                "M2 did not drain active writer instruction {} (ROB {}, completed={})",
                active.meta.instruction_id, active.rob_id, active.completed
            ));
        }
        Ok(())
    }
}

static MONITOR: OnceLock<Mutex<Option<M2Monitor>>> = OnceLock::new();

fn monitor() -> &'static Mutex<Option<M2Monitor>> {
    MONITOR.get_or_init(|| Mutex::new(None))
}

pub(crate) fn init(log_dir: &Path, config: Option<BankDigestConfig>) -> io::Result<()> {
    let value = config.map(|config| M2Monitor::new(log_dir, config)).transpose()?;
    *monitor().lock().unwrap() = value;
    Ok(())
}

pub(crate) fn record_instruction(event: &ITraceEvent) {
    if let Some(monitor) = monitor().lock().unwrap().as_mut() {
        monitor.record_instruction(event);
    }
}

pub(crate) fn record_memory(event: &MTraceEvent) {
    if let Some(monitor) = monitor().lock().unwrap().as_mut() {
        monitor.record_memory(event);
    }
}

pub fn poll() -> Result<(), String> {
    if let Some(monitor) = monitor().lock().unwrap().as_mut() {
        monitor.poll(false)
    } else {
        Ok(())
    }
}

pub fn finish() -> Result<(), String> {
    if let Some(monitor) = monitor().lock().unwrap().as_mut() {
        monitor.finish()
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instruction(is_issue: u8, rob_id: u32, bank_enable: u8) -> ITraceEvent {
        ITraceEvent {
            is_issue,
            rob_id,
            domain_id: 0,
            funct: 64,
            pc: 0x8000_1000,
            rs1: 0,
            rs2: 0,
            bank_enable,
        }
    }

    fn write(vbank_id: u32, group_id: u32, addr: u32, data_lo: u64) -> MTraceEvent {
        MTraceEvent {
            is_write: 1,
            is_shared: 0,
            channel: 0,
            hart_id: 0,
            vbank_id,
            pbank_id: 3,
            group_id,
            addr,
            data_lo,
            data_hi: 0,
        }
    }

    fn test_monitor() -> M2Monitor {
        let dir = std::env::temp_dir();
        M2Monitor::new(&dir, BankDigestConfig::new(64, 16)).unwrap()
    }

    #[test]
    fn actual_write_selects_the_single_bank_and_preserves_idempotent_writes() {
        let mut monitor = test_monitor();
        monitor.record_instruction(&instruction(2, 7, 2));
        monitor.record_memory(&write(5, 1, 2, 0));
        monitor.record_instruction(&instruction(0, 7, 2));
        monitor.poll(false).unwrap();
        assert!(monitor.active_writer.is_some());
        monitor.poll(false).unwrap();
        assert!(monitor.active_writer.is_none());
        assert_eq!(monitor.next_line, 1);
    }

    #[test]
    fn read_only_instruction_does_not_create_a_writer() {
        let mut monitor = test_monitor();
        monitor.record_instruction(&instruction(2, 7, 1));
        monitor.record_instruction(&instruction(0, 7, 1));
        monitor.finish().unwrap();
        assert_eq!(monitor.next_line, 0);
    }

    #[test]
    fn multiple_logical_banks_are_rejected_in_m2() {
        let mut monitor = test_monitor();
        monitor.record_instruction(&instruction(2, 7, 2));
        monitor.record_memory(&write(5, 0, 0, 1));
        monitor.record_memory(&write(5, 1, 0, 2));
        assert!(monitor.poll(false).unwrap_err().contains("multiple logical banks"));
    }
}
