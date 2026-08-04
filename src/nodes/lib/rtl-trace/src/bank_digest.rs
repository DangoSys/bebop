use crate::itrace::ITraceEvent;
use crate::mtrace::{MTraceEvent, MTraceIssueEvent};
use crate::state;
use bebop_bank_hash::{
    bank_hash, submit_runtime_bank_digest, BankDigestRecord, BankHashEventClass, BankHashSource, BankHashTime,
    LogicalBankId,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

const RTL_RECORD_FILE: &str = "rtl_bank_digest.ndjson";
// The issue, arrival, and completion DPI blocks each register their payload.
// Waiting one host poll after completion drains events generated no later than
// that completion without relying on a guessed execution latency.
const COMPLETION_DPI_GRACE_POLLS: u64 = 1;

/// Geometry required to reconstruct a complete private bank from actual RTL
/// SPM-arrival writes. M4 currently supports the 128-bit private-bank port.
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BankDigestStatus {
    pub in_flight_instructions: usize,
    pub pending_bank_updates: usize,
    pub issued_writes: u64,
    pub arrived_writes: u64,
}

impl BankDigestStatus {
    pub fn is_drained(self) -> bool {
        self.in_flight_instructions == 0 && self.pending_bank_updates == 0
    }
}

#[derive(Clone, Debug)]
struct InstructionMeta {
    instruction_id: u64,
    funct7: u32,
    pc: u64,
}

#[derive(Clone, Debug, Default)]
struct BankUpdate {
    issued: u64,
    arrived: u64,
    physical_bank_id: Option<u32>,
    emitted: bool,
}

#[derive(Clone, Debug)]
struct Producer {
    meta: InstructionMeta,
    completed_at_poll: Option<u64>,
    updates: BTreeMap<LogicalBankId, BankUpdate>,
}

struct M4Monitor {
    config: BankDigestConfig,
    physical_banks: BTreeMap<u32, Vec<u8>>,
    producers: BTreeMap<u32, Producer>,
    // Verilator may invoke the combinational issue DPI before the sequential
    // instruction-allocation DPI in the same eval. Keep those real write
    // transactions in the scoreboard until their ROB metadata arrives.
    pending_issues: BTreeMap<u32, BTreeMap<LogicalBankId, u64>>,
    pending_arrivals: BTreeMap<u32, Vec<MTraceEvent>>,
    // BootRom commands have pc=0 and are not part of the guest instruction
    // stream executed by the BEMU golden model. Their zero-fill writes must
    // not consume guest InstIDs or enter the online comparator.
    boot_robs: BTreeSet<u32>,
    next_instruction_id: u64,
    poll_count: u64,
    next_line: u64,
    output: File,
    error: Option<String>,
}

impl M4Monitor {
    fn new(log_dir: &Path, config: BankDigestConfig) -> io::Result<Self> {
        if config.bank_size == 0 || config.row_bytes != 16 || config.bank_size % config.row_bytes != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "M4 requires a non-zero bank size divisible by the 16-byte RTL trace row: bank_size={} row_bytes={}",
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
            physical_banks: BTreeMap::new(),
            producers: BTreeMap::new(),
            pending_issues: BTreeMap::new(),
            pending_arrivals: BTreeMap::new(),
            boot_robs: BTreeSet::new(),
            next_instruction_id: 0,
            poll_count: 0,
            next_line: 0,
            output,
            error: None,
        })
    }

    fn fail(&mut self, message: impl Into<String>) {
        if self.error.is_none() {
            self.error = Some(format!("M4 attribution error: {}", message.into()));
        }
    }

    fn record_instruction(&mut self, event: &ITraceEvent) {
        if event.pc == 0 {
            match event.is_issue {
                2 => {
                    self.pending_issues.remove(&event.rob_id);
                    self.pending_arrivals.remove(&event.rob_id);
                    self.boot_robs.insert(event.rob_id);
                }
                0 => {
                    self.boot_robs.remove(&event.rob_id);
                }
                _ => {}
            }
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
        let mut producer = Producer {
            meta: InstructionMeta {
                instruction_id: self.next_instruction_id,
                funct7: event.funct,
                pc: event.pc,
            },
            completed_at_poll: None,
            updates: BTreeMap::new(),
        };

        if let Some(pending) = self.pending_issues.remove(&event.rob_id) {
            for (bank_id, issued) in pending {
                producer.updates.entry(bank_id).or_default().issued = issued;
            }
        }

        if self.producers.insert(event.rob_id, producer).is_some() {
            self.fail(format!(
                "ROB {} was reused before its Bank-Stable updates drained",
                event.rob_id
            ));
        }
        if let Some(pending) = self.pending_arrivals.remove(&event.rob_id) {
            for arrival in pending {
                self.record_arrival(&arrival);
            }
        }
    }

    fn complete(&mut self, rob_id: u32) {
        let Some(producer) = self.producers.get_mut(&rob_id) else {
            self.fail(format!("completion without allocation for ROB {rob_id}"));
            return;
        };
        if producer.completed_at_poll.replace(self.poll_count).is_some() {
            self.fail(format!("duplicate completion for ROB {rob_id}"));
        }
    }

    fn record_issue(&mut self, event: &MTraceIssueEvent) {
        let _hart_id = event.hart_id;
        if event.is_shared != 0 || self.boot_robs.contains(&event.rob_id) {
            return;
        }
        let bank_id = LogicalBankId::new(event.vbank_id, event.group_id);
        let Some(producer) = self.producers.get_mut(&event.rob_id) else {
            *self
                .pending_issues
                .entry(event.rob_id)
                .or_default()
                .entry(bank_id)
                .or_default() += 1;
            return;
        };
        if producer.updates.entry(bank_id).or_default().emitted {
            if self.error.is_none() {
                self.error = Some(format!(
                    "M4 attribution error: write issue arrived after Stable record for instruction {} bank ({},{})",
                    producer.meta.instruction_id, bank_id.vbank_id, bank_id.group_id
                ));
            }
            return;
        }
        producer.updates.get_mut(&bank_id).expect("update exists").issued += 1;
    }

    fn record_arrival(&mut self, event: &MTraceEvent) {
        if event.is_write == 0 || event.is_shared != 0 || self.boot_robs.contains(&event.rob_id) {
            return;
        }
        let bank_id = LogicalBankId::new(event.vbank_id, event.group_id);
        let instruction_id;
        {
            let Some(producer) = self.producers.get_mut(&event.rob_id) else {
                self.pending_arrivals.entry(event.rob_id).or_default().push(*event);
                return;
            };
            instruction_id = producer.meta.instruction_id;
            let update = producer.updates.entry(bank_id).or_default();
            if update.emitted {
                self.fail(format!(
                    "SPM write arrived after Stable record for instruction {} bank ({},{})",
                    instruction_id, bank_id.vbank_id, bank_id.group_id
                ));
                return;
            }
            if let Some(previous) = update.physical_bank_id {
                if previous != event.pbank_id {
                    self.fail(format!(
                        "instruction {} logical bank ({},{}) changed physical bank from {} to {}",
                        instruction_id, bank_id.vbank_id, bank_id.group_id, previous, event.pbank_id
                    ));
                    return;
                }
            } else {
                update.physical_bank_id = Some(event.pbank_id);
            }
            update.arrived += 1;
        }

        let Some(offset) = (event.addr as usize).checked_mul(self.config.row_bytes) else {
            self.fail(format!("bank row address overflow: {}", event.addr));
            return;
        };
        if offset + self.config.row_bytes > self.config.bank_size {
            self.fail(format!(
                "bank row {} exceeds bank size {} for instruction {}",
                event.addr, self.config.bank_size, instruction_id
            ));
            return;
        }
        // Shadow the physical SRAM, not the logical mapping. Reallocating a
        // logical bank does not itself prove that RTL cleared the underlying
        // storage; preserving the physical contents lets full-bank DiffTest
        // expose such initialization bugs and preserves masked-off bytes.
        let bank = self
            .physical_banks
            .entry(event.pbank_id)
            .or_insert_with(|| vec![0; self.config.bank_size]);
        let mut row = [0u8; 16];
        row[..8].copy_from_slice(&event.data_lo.to_le_bytes());
        row[8..].copy_from_slice(&event.data_hi.to_le_bytes());
        for (lane, byte) in row.into_iter().enumerate() {
            if event.write_mask & (1 << lane) != 0 {
                bank[offset + lane] = byte;
            }
        }
    }

    fn poll(&mut self, force: bool) -> Result<(), String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        self.poll_count = self.poll_count.wrapping_add(1);
        self.emit_stable_updates(force)?;
        self.retire_drained_producers(force);
        if let Some(error) = &self.error {
            Err(error.clone())
        } else {
            Ok(())
        }
    }

    fn emit_stable_updates(&mut self, force: bool) -> Result<(), String> {
        let mut ready = Vec::new();
        for (&rob_id, producer) in &self.producers {
            let Some(completed_at) = producer.completed_at_poll else {
                continue;
            };
            if !force && self.poll_count.saturating_sub(completed_at) < COMPLETION_DPI_GRACE_POLLS {
                continue;
            }
            for (&bank_id, update) in &producer.updates {
                if !update.emitted && update.issued != 0 && update.issued == update.arrived {
                    ready.push((rob_id, bank_id));
                }
            }
        }

        for (rob_id, bank_id) in ready {
            let producer = self.producers.get(&rob_id).expect("ready producer exists");
            let update = producer.updates.get(&bank_id).expect("ready update exists");
            let meta = producer.meta.clone();
            let physical_bank_id = update.physical_bank_id;
            let physical_bank_id = physical_bank_id.expect("arrived update has a physical bank id");
            let bytes = self
                .physical_banks
                .get(&physical_bank_id)
                .expect("arrived update has a physical shadow bank");
            self.next_line = self.next_line.wrapping_add(1);
            let record = BankDigestRecord::new(
                BankHashSource::Rtl,
                meta.instruction_id,
                bank_id,
                Some(physical_bank_id),
                bank_hash(bytes),
                meta.funct7,
                format!("funct7_{}", meta.funct7),
                BankHashEventClass::BankDataWrite,
                BankHashTime::Cycle(state::rtl_clk()),
                Some(meta.pc),
                Some(format!("{RTL_RECORD_FILE}:{}", self.next_line)),
            );
            let line = record.to_ndjson().map_err(|error| error.to_string())?;
            self.output
                .write_all(line.as_bytes())
                .map_err(|error| error.to_string())?;
            self.output.flush().map_err(|error| error.to_string())?;
            submit_runtime_bank_digest(&record);
            self.producers
                .get_mut(&rob_id)
                .expect("ready producer exists")
                .updates
                .get_mut(&bank_id)
                .expect("ready update exists")
                .emitted = true;
        }
        Ok(())
    }

    fn retire_drained_producers(&mut self, force: bool) {
        let poll_count = self.poll_count;
        self.producers.retain(|_, producer| {
            let Some(completed_at) = producer.completed_at_poll else {
                return true;
            };
            let grace_done = force || poll_count.saturating_sub(completed_at) >= COMPLETION_DPI_GRACE_POLLS;
            let updates_done = producer.updates.values().all(|update| update.emitted);
            !(grace_done && updates_done)
        });
    }

    fn status(&self) -> BankDigestStatus {
        BankDigestStatus {
            in_flight_instructions: self.producers.len()
                + self
                    .pending_issues
                    .keys()
                    .chain(self.pending_arrivals.keys())
                    .collect::<BTreeSet<_>>()
                    .len(),
            pending_bank_updates: self
                .producers
                .values()
                .flat_map(|producer| producer.updates.values())
                .filter(|update| !update.emitted)
                .count()
                + self.pending_issues.values().map(BTreeMap::len).sum::<usize>()
                + self.pending_arrivals.values().map(Vec::len).sum::<usize>(),
            issued_writes: self
                .producers
                .values()
                .flat_map(|producer| producer.updates.values())
                .map(|update| update.issued)
                .sum::<u64>()
                + self
                    .pending_issues
                    .values()
                    .flat_map(|updates| updates.values())
                    .sum::<u64>(),
            arrived_writes: self
                .producers
                .values()
                .flat_map(|producer| producer.updates.values())
                .map(|update| update.arrived)
                .sum::<u64>()
                + self
                    .pending_arrivals
                    .values()
                    .map(|events| events.len() as u64)
                    .sum::<u64>(),
        }
    }

    fn finish(&mut self) -> Result<(), String> {
        self.poll(true)?;
        if self.producers.is_empty() && self.pending_issues.is_empty() && self.pending_arrivals.is_empty() {
            return Ok(());
        }
        if !self.pending_issues.is_empty() || !self.pending_arrivals.is_empty() {
            let mut details = self
                .pending_issues
                .iter()
                .flat_map(|(rob_id, updates)| {
                    updates.iter().map(move |(bank_id, issued)| {
                        format!(
                            "rob={} bank=({},{}) issued={}",
                            rob_id, bank_id.vbank_id, bank_id.group_id, issued
                        )
                    })
                })
                .collect::<Vec<_>>();
            let mut arrival_counts = BTreeMap::<(u32, LogicalBankId), u64>::new();
            for (&rob_id, arrivals) in &self.pending_arrivals {
                for arrival in arrivals {
                    *arrival_counts
                        .entry((rob_id, LogicalBankId::new(arrival.vbank_id, arrival.group_id)))
                        .or_default() += 1;
                }
            }
            details.extend(arrival_counts.into_iter().map(|((rob_id, bank_id), arrivals)| {
                format!(
                    "rob={} bank=({},{}) arrivals={}",
                    rob_id, bank_id.vbank_id, bank_id.group_id, arrivals
                )
            }));
            return Err(format!(
                "M4 write transactions never received instruction allocation metadata: {}",
                details.join("; ")
            ));
        }
        let details = self
            .producers
            .iter()
            .flat_map(|(rob_id, producer)| {
                producer.updates.iter().map(move |(bank_id, update)| {
                    format!(
                        "inst={} rob={} bank=({},{}) issued={} arrived={} completed={} emitted={}",
                        producer.meta.instruction_id,
                        rob_id,
                        bank_id.vbank_id,
                        bank_id.group_id,
                        update.issued,
                        update.arrived,
                        producer.completed_at_poll.is_some(),
                        update.emitted
                    )
                })
            })
            .collect::<Vec<_>>()
            .join("; ");
        Err(format!("M4 Bank-Stable Boundary did not drain: {details}"))
    }
}

static MONITOR: OnceLock<Mutex<Option<M4Monitor>>> = OnceLock::new();

fn monitor() -> &'static Mutex<Option<M4Monitor>> {
    MONITOR.get_or_init(|| Mutex::new(None))
}

pub(crate) fn init(log_dir: &Path, config: Option<BankDigestConfig>) -> io::Result<()> {
    let value = config.map(|config| M4Monitor::new(log_dir, config)).transpose()?;
    *monitor().lock().unwrap() = value;
    Ok(())
}

pub(crate) fn record_instruction(event: &ITraceEvent) {
    if let Some(monitor) = monitor().lock().unwrap().as_mut() {
        monitor.record_instruction(event);
    }
}

pub(crate) fn record_write_issue(event: &MTraceIssueEvent) {
    if let Some(monitor) = monitor().lock().unwrap().as_mut() {
        monitor.record_issue(event);
    }
}

pub(crate) fn record_memory(event: &MTraceEvent) {
    if let Some(monitor) = monitor().lock().unwrap().as_mut() {
        monitor.record_arrival(event);
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

pub fn status() -> BankDigestStatus {
    monitor()
        .lock()
        .unwrap()
        .as_ref()
        .map(M4Monitor::status)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn instruction(is_issue: u8, rob_id: u32, funct: u32) -> ITraceEvent {
        ITraceEvent {
            is_issue,
            rob_id,
            domain_id: 0,
            funct,
            pc: 0x8000_1000 + u64::from(rob_id) * 4,
            rs1: 0,
            rs2: 0,
            bank_enable: 2,
        }
    }

    fn issue(rob_id: u32, vbank_id: u32, group_id: u32) -> MTraceIssueEvent {
        MTraceIssueEvent {
            is_shared: 0,
            hart_id: 0,
            rob_id,
            vbank_id,
            group_id,
        }
    }

    fn arrival(rob_id: u32, vbank_id: u32, group_id: u32, addr: u32, data_lo: u64) -> MTraceEvent {
        MTraceEvent {
            is_write: 1,
            is_shared: 0,
            channel: 0,
            hart_id: 0,
            rob_id,
            vbank_id,
            pbank_id: vbank_id + group_id,
            group_id,
            addr,
            write_mask: 0xffff,
            data_lo,
            data_hi: 0,
        }
    }

    fn test_monitor() -> M4Monitor {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("bebop-m3-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        M4Monitor::new(&dir, BankDigestConfig::new(64, 16)).unwrap()
    }

    #[test]
    fn concurrent_writers_reach_independent_stable_boundaries() {
        let mut monitor = test_monitor();
        monitor.record_instruction(&instruction(2, 1, 64));
        monitor.record_instruction(&instruction(2, 2, 65));
        monitor.record_issue(&issue(1, 5, 0));
        monitor.record_issue(&issue(2, 6, 0));
        monitor.record_arrival(&arrival(2, 6, 0, 0, 22));
        monitor.record_instruction(&instruction(0, 2, 65));
        monitor.record_arrival(&arrival(1, 5, 0, 0, 11));
        monitor.record_instruction(&instruction(0, 1, 64));
        monitor.poll(false).unwrap();
        assert_eq!(monitor.next_line, 2);
        assert!(monitor.status().is_drained());
    }

    #[test]
    fn issue_before_allocation_is_bound_by_rob_scoreboard() {
        let mut monitor = test_monitor();
        monitor.record_issue(&issue(1, 5, 0));
        monitor.record_arrival(&arrival(1, 5, 0, 0, 11));
        assert_eq!(monitor.status().issued_writes, 1);
        assert_eq!(monitor.status().arrived_writes, 1);
        monitor.record_instruction(&instruction(2, 1, 64));
        monitor.record_instruction(&instruction(0, 1, 64));
        monitor.finish().unwrap();
        assert_eq!(monitor.next_line, 1);
        assert!(monitor.status().is_drained());
    }

    #[test]
    fn boot_rom_writes_do_not_enter_guest_scoreboard() {
        let mut monitor = test_monitor();
        monitor.record_issue(&issue(1, 0, 0));
        monitor.record_arrival(&arrival(1, 0, 0, 0, 0));
        let mut boot_alloc = instruction(2, 1, 33);
        boot_alloc.pc = 0;
        monitor.record_instruction(&boot_alloc);
        monitor.record_arrival(&arrival(1, 0, 0, 1, 0));
        let mut boot_complete = instruction(0, 1, 33);
        boot_complete.pc = 0;
        monitor.record_instruction(&boot_complete);
        monitor.finish().unwrap();
        assert!(monitor.status().is_drained());
        assert_eq!(monitor.next_line, 0);
    }

    #[test]
    fn one_instruction_can_emit_multiple_logical_bank_records() {
        let mut monitor = test_monitor();
        monitor.record_instruction(&instruction(2, 7, 65));
        for group_id in 0..2 {
            monitor.record_issue(&issue(7, 5, group_id));
            monitor.record_arrival(&arrival(7, 5, group_id, 0, u64::from(group_id + 1)));
        }
        monitor.record_instruction(&instruction(0, 7, 65));
        monitor.finish().unwrap();
        assert_eq!(monitor.next_line, 2);
    }

    #[test]
    fn completion_waits_until_issued_and_arrived_counts_match() {
        let mut monitor = test_monitor();
        monitor.record_instruction(&instruction(2, 3, 64));
        monitor.record_issue(&issue(3, 5, 0));
        monitor.record_instruction(&instruction(0, 3, 64));
        monitor.poll(false).unwrap();
        assert_eq!(monitor.next_line, 0);
        monitor.record_arrival(&arrival(3, 5, 0, 0, 9));
        monitor.poll(false).unwrap();
        assert_eq!(monitor.next_line, 1);
        assert!(monitor.status().is_drained());
    }

    #[test]
    fn missing_arrival_is_reported_at_finish() {
        let mut monitor = test_monitor();
        monitor.record_instruction(&instruction(2, 4, 64));
        monitor.record_issue(&issue(4, 5, 0));
        monitor.record_instruction(&instruction(0, 4, 64));
        let error = monitor.finish().unwrap_err();
        assert!(error.contains("issued=1 arrived=0"));
    }

    #[test]
    fn read_only_or_conditionally_unwritten_instruction_has_no_record() {
        let mut monitor = test_monitor();
        monitor.record_instruction(&instruction(2, 8, 53));
        monitor.record_instruction(&instruction(0, 8, 53));
        monitor.finish().unwrap();
        assert_eq!(monitor.next_line, 0);
    }

    #[test]
    fn masked_arrival_preserves_unwritten_bytes_in_complete_bank_shadow() {
        let mut monitor = test_monitor();
        monitor.record_instruction(&instruction(2, 9, 64));
        monitor.record_issue(&issue(9, 5, 0));
        let mut first = arrival(9, 5, 0, 0, 0x0807_0605_0403_0201);
        first.write_mask = 0xffff;
        first.data_hi = 0x100f_0e0d_0c0b_0a09;
        monitor.record_arrival(&first);
        monitor.record_instruction(&instruction(0, 9, 64));
        monitor.poll(false).unwrap();

        monitor.record_instruction(&instruction(2, 10, 64));
        monitor.record_issue(&issue(10, 5, 0));
        let mut masked = arrival(10, 5, 0, 0, 0xffff_ffff_ffff_ffff);
        masked.write_mask = 0x0003;
        masked.data_hi = u64::MAX;
        monitor.record_arrival(&masked);
        assert_eq!(
            &monitor.physical_banks[&5][..16],
            &[0xff, 0xff, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
    }

    #[test]
    fn physical_bank_reuse_preserves_unwritten_bytes() {
        let mut monitor = test_monitor();
        monitor.record_instruction(&instruction(2, 11, 64));
        monitor.record_issue(&issue(11, 5, 0));
        let mut first = arrival(11, 5, 0, 0, 0x0807_0605_0403_0201);
        first.pbank_id = 3;
        monitor.record_arrival(&first);

        monitor.record_instruction(&instruction(2, 12, 64));
        monitor.record_issue(&issue(12, 6, 0));
        let mut reused = arrival(12, 6, 0, 0, 0xffff_ffff_ffff_ffff);
        reused.pbank_id = 3;
        reused.write_mask = 0x0001;
        monitor.record_arrival(&reused);

        assert_eq!(&monitor.physical_banks[&3][..8], &[0xff, 2, 3, 4, 5, 6, 7, 8]);
    }
}
