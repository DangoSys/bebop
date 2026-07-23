// Trace logging (NDJSON format)

use crate::ffi::{
    verilator_flip_private_bank_bit, verilator_hash_private_bank, verilator_read_rob_bank_access,
    verilator_resolve_private_bank_mask, VerilatorTop,
};
use bebop_bank_hash::{
    report_runtime_bank_difftest_failure, submit_runtime_bank_boundary, BankDigest, BankHashEventClass, BankHashPacket,
    BankHashPacketId, BankHashSource, BankHashTime, BankVersionRef, CanonicalBankHashPacket,
    InstructionBankBoundaryPacket, BANK_NUM,
};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::OnceLock;

static VERILATOR_TOP: OnceLock<AtomicPtr<VerilatorTop>> = OnceLock::new();

thread_local! {
    // Verilator evaluation and all trace DPI callbacks run on one simulation
    // thread. Keep the hot DiffTest state on that thread: a process-wide
    // Mutex here used to be acquired twice per RTL cycle even when no Bank
    // boundary was pending.
    // Set only by a DPI callback that completed an architectural writer. This
    // is the per-eval fast-path gate: ordinary RTL cycles never borrow or scan
    // the monitor.
    // Bit 0 = enabled, bit 1 = work ready, bit 2 = failure since the previous
    // eval. Keeping all hot status in one Cell gives enabled and disabled runs
    // the same single read on every ordinary eval.
    static RTL_BANK_HASH_STATE: Cell<u8> = const { Cell::new(0) };
    static RTL_CLK: Cell<u64> = const { Cell::new(0) };
    static RTL_BANK_STABILITY_MONITOR: RefCell<BankStabilityMonitor> = RefCell::new(BankStabilityMonitor::new());
    static RTL_BTRACE_STATE: RefCell<BtraceState> = RefCell::new(BtraceState::new());
    static TRACE_FILE: RefCell<Option<File>> = const { RefCell::new(None) };
    static RTL_BANK_HASH_FILE: RefCell<Option<File>> = const { RefCell::new(None) };
    static RTL_BTRACE_LOG_FILE: RefCell<Option<File>> = const { RefCell::new(None) };
    static ENABLE_ITRACE: Cell<bool> = const { Cell::new(false) };
    static ENABLE_MTRACE: Cell<bool> = const { Cell::new(false) };
    static ENABLE_PMCTRACE: Cell<bool> = const { Cell::new(false) };
    static ENABLE_CTRACE: Cell<bool> = const { Cell::new(false) };
    static ENABLE_BANKTRACE: Cell<bool> = const { Cell::new(false) };
    static RTL_SPM_FAULT_STATE: RefCell<RuntimeSpmFaultState> =
        const { RefCell::new(RuntimeSpmFaultState::new()) };
}

fn record_rtl_difftest_failure() {
    RTL_BANK_HASH_STATE.with(|state| state.set(state.get() | 4));
    // Preserve the process-wide signal for the legacy background comparator.
    report_runtime_bank_difftest_failure();
}

fn with_rtl_bank_stability_monitor<R>(f: impl FnOnce(&mut BankStabilityMonitor) -> R) -> R {
    RTL_BANK_STABILITY_MONITOR.with(|monitor| f(&mut monitor.borrow_mut()))
}

fn rtl_bank_hash_enabled() -> bool {
    RTL_BANK_HASH_STATE.with(Cell::get) & 1 != 0
}

#[derive(Clone, Copy, Debug)]
pub struct SpmFaultConfig {
    pub semantic_seq: Option<u64>,
    pub byte_offset: u32,
    pub bit: u8,
}

#[derive(Clone, Copy, Debug)]
struct RuntimeSpmFaultState {
    config: Option<SpmFaultConfig>,
    attempted: bool,
    injected: bool,
}

impl RuntimeSpmFaultState {
    const fn new() -> Self {
        Self {
            config: None,
            attempted: false,
            injected: false,
        }
    }

    fn reset(&mut self, config: Option<SpmFaultConfig>) {
        self.config = config;
        self.attempted = false;
        self.injected = false;
    }
}

fn maybe_inject_spm_fault(semantic_seq: u64, logical_bank: u32, physical_bank: u32) {
    let Some(config) = RTL_SPM_FAULT_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let config = state.config?;
        if state.attempted || config.semantic_seq.is_some_and(|seq| seq != semantic_seq) {
            return None;
        }
        state.attempted = true;
        Some(config)
    }) else {
        return;
    };

    let top = get_verilator_top().load(Ordering::SeqCst);
    let injected = !top.is_null()
        && unsafe { verilator_flip_private_bank_bit(top, physical_bank, config.byte_offset, config.bit) };
    RTL_SPM_FAULT_STATE.with(|state| state.borrow_mut().injected = injected);
    write_trace(&format!(
        r#"{{"type":"spm_fault_injection","result":"{}","clk":{},"semantic_seq":{},"logical_bank":{},"physical_bank":{},"byte_offset":{},"bit":{}}}"#,
        if injected { "INJECTED" } else { "FAILED" },
        rtl_clk(),
        semantic_seq,
        logical_bank,
        physical_bank,
        config.byte_offset,
        config.bit
    ));
}

#[derive(Debug)]
struct BtraceState {
    raw_line: u64,
}

impl BtraceState {
    fn new() -> Self {
        Self { raw_line: 0 }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn next_raw_line(&mut self) -> u64 {
        self.raw_line = self.raw_line.wrapping_add(1);
        self.raw_line
    }
}

#[derive(Clone, Debug)]
struct StableHashTask {
    instruction_id: u64,
    semantic_seq: u64,
    bank_id: u32,
    bank_version: u32,
    hash: u64,
    funct7: u32,
    op_type: String,
    cycle: u64,
    pc: u64,
}

#[derive(Clone, Debug)]
struct StableInstructionTask {
    boundary: InstructionBankBoundaryPacket,
    hashes: Vec<StableHashTask>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WriterSource {
    Dma,
    VectorUnit,
    WritebackUnit,
}

impl WriterSource {
    fn for_domain(domain_id: u32) -> Self {
        match domain_id {
            1 => Self::Dma,
            3 => Self::VectorUnit,
            _ => Self::WritebackUnit,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Dma => "dma",
            Self::VectorUnit => "vector_unit",
            Self::WritebackUnit => "writeback_unit",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AffectedBankSource {
    RtlPrivateMapping,
    SoftwareMirrorFallback,
}

impl AffectedBankSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::RtlPrivateMapping => "rtl_private_mapping",
            Self::SoftwareMirrorFallback => "software_mirror_fallback",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct RtlBankConfig {
    allocated: bool,
    cols: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct RtlBankMapEntry {
    valid: bool,
    vbank_id: u32,
    group_id: u32,
}

#[derive(Clone, Debug)]
struct RtlBankMap {
    slots: [RtlBankMapEntry; BANK_NUM],
}

impl RtlBankMap {
    fn new() -> Self {
        Self {
            slots: [RtlBankMapEntry::default(); BANK_NUM],
        }
    }

    fn delete_vbank(&mut self, vbank_id: u32) {
        for slot in &mut self.slots {
            if slot.valid && slot.vbank_id == vbank_id {
                *slot = RtlBankMapEntry::default();
            }
        }
    }

    fn first_free_pbank(&self) -> Option<usize> {
        self.slots.iter().position(|slot| !slot.valid)
    }

    fn bind_group(&mut self, pbank_id: usize, vbank_id: u32, group_id: u32) {
        if pbank_id >= BANK_NUM {
            return;
        }

        self.slots[pbank_id] = RtlBankMapEntry {
            valid: true,
            vbank_id,
            group_id,
        };
    }

    fn resolve_group(&self, vbank_id: u32, group_id: u32) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| slot.valid && slot.vbank_id == vbank_id && slot.group_id == group_id)
    }
}

#[derive(Clone, Debug)]
struct ProducerMeta {
    rob_id: u64,
    instruction_id: u64,
    semantic_seq: u64,
    funct7: u32,
    op_type: String,
    pc: u64,
    bank_enable: u8,
    target_vbank_id: Option<u32>,
    affected_bank_set: BTreeSet<usize>,
    expected_logical_banks: BTreeSet<u32>,
    actual_logical_banks: BTreeSet<u32>,
    logical_to_physical: BTreeMap<u32, usize>,
    reads: BTreeSet<BankVersionRef>,
    affected_bank_source: AffectedBankSource,
    alloc_cycle: u64,
    complete_cycle: Option<u64>,
    outstanding_writes: u64,
    write_end: bool,
    cancelled: bool,
    explicit_protocol: bool,
    writer_source: WriterSource,
}

#[derive(Clone, Debug)]
struct PendingHashTask {
    task_id: u64,
    instruction_id: u64,
    bank_id: usize,
    funct7: u32,
    op_type: String,
    cycle: u64,
    pc: u64,
    bank_enable: u8,
    alloc_cycle: u64,
    complete_cycle: u64,
    stable_cycle: Option<u64>,
    observed_write_count: u64,
    writer_source: WriterSource,
    affected_bank_source: AffectedBankSource,
}

#[derive(Clone, Copy, Debug)]
struct BankStabilitySnapshot {
    pending_same_bank_writes: u32,
}

#[derive(Debug)]
struct BankStabilityMonitor {
    producer_metadata: BTreeMap<u64, ProducerMeta>,
    retired_boot_rob_ids: BTreeSet<u64>,
    bank_cfgs: [RtlBankConfig; BANK_NUM],
    bank_map: RtlBankMap,
    write_request_counts: [u64; BANK_NUM],
    /// Per-Bank writer scoreboard. Each entry records every issued operation
    /// which may still modify that complete physical Bank.
    bank_writers: [BTreeMap<u64, WriterSource>; BANK_NUM],
    bank_versions: BTreeMap<u32, u32>,
    completed_this_eval: Vec<u64>,
    task_count: u64,
    next_op_id: u64,
    bank_target_checks: u64,
    bank_target_mismatches: u64,
    bank_write_attribution_errors: u64,
}

impl BankStabilityMonitor {
    fn new() -> Self {
        Self {
            producer_metadata: BTreeMap::new(),
            retired_boot_rob_ids: BTreeSet::new(),
            bank_cfgs: [RtlBankConfig::default(); BANK_NUM],
            bank_map: RtlBankMap::new(),
            write_request_counts: [0; BANK_NUM],
            bank_writers: std::array::from_fn(|_| BTreeMap::new()),
            bank_versions: BTreeMap::new(),
            completed_this_eval: Vec::new(),
            task_count: 0,
            next_op_id: 0,
            bank_target_checks: 0,
            bank_target_mismatches: 0,
            bank_write_attribution_errors: 0,
        }
    }

    fn reset(&mut self) {
        self.producer_metadata.clear();
        self.retired_boot_rob_ids.clear();
        self.bank_cfgs = [RtlBankConfig::default(); BANK_NUM];
        self.bank_map = RtlBankMap::new();
        self.write_request_counts = [0; BANK_NUM];
        self.bank_writers = std::array::from_fn(|_| BTreeMap::new());
        self.bank_versions.clear();
        self.completed_this_eval.clear();
        self.task_count = 0;
        self.next_op_id = 0;
        self.bank_target_checks = 0;
        self.bank_target_mismatches = 0;
        self.bank_write_attribution_errors = 0;
    }

    fn record_allocation(&mut self, event: &ITraceEvent, cycle: u64) {
        if event.is_issue != 2 {
            return;
        }

        if event.funct == 32 {
            self.apply_mset(event.rs1, event.rs2);
        }
        self.retired_boot_rob_ids.remove(&(event.rob_id as u64));
        let instruction_id = if event.pc == 0 {
            0
        } else {
            self.next_op_id = self.next_op_id.wrapping_add(1);
            self.next_op_id
        };
        if let Some(previous) = self.producer_metadata.get(&(event.rob_id as u64)) {
            self.protocol_error(
                event.rob_id as u64,
                format!(
                    "producer_tag_reused_before_release previous_instruction_id={}",
                    previous.instruction_id
                ),
            );
            return;
        }
        self.producer_metadata.insert(
            event.rob_id as u64,
            ProducerMeta {
                rob_id: event.rob_id as u64,
                instruction_id,
                semantic_seq: instruction_id,
                funct7: event.funct,
                op_type: format!("funct7_{}", event.funct),
                pc: event.pc,
                bank_enable: event.bank_enable,
                target_vbank_id: None,
                affected_bank_set: BTreeSet::new(),
                expected_logical_banks: BTreeSet::new(),
                actual_logical_banks: BTreeSet::new(),
                logical_to_physical: BTreeMap::new(),
                reads: BTreeSet::new(),
                affected_bank_source: AffectedBankSource::RtlPrivateMapping,
                alloc_cycle: cycle,
                complete_cycle: None,
                outstanding_writes: 0,
                write_end: false,
                cancelled: false,
                explicit_protocol: false,
                writer_source: WriterSource::for_domain(event.domain_id),
            },
        );
    }

    fn protocol_error(&mut self, rob_id: u64, reason: impl AsRef<str>) {
        self.bank_write_attribution_errors = self.bank_write_attribution_errors.wrapping_add(1);
        write_trace(&format!(
            r#"{{"type":"bank_protocol","result":"ERROR","clk":{},"rob_id":{},"reason":"{}"}}"#,
            rtl_clk(),
            rob_id,
            reason.as_ref()
        ));
        record_rtl_difftest_failure();
    }

    fn record_issue(&mut self, event: &ITraceEvent) {
        if event.is_issue != 1 {
            return;
        }
        let rob_id = event.rob_id as u64;
        let Some(_) = self.producer_metadata.get(&rob_id) else {
            self.protocol_error(rob_id, "issue_for_unknown_producer");
            return;
        };
        let Some(access) = read_rtl_decoded_bank_access(event.rob_id) else {
            self.protocol_error(rob_id, "decoded_bank_access_unavailable");
            return;
        };
        write_trace(&format!(
            r#"{{"type":"bank_access_decode","clk":{},"rob_id":{},"rd0_valid":{},"rd0_vbank_id":{},"rd1_valid":{},"rd1_vbank_id":{},"wr_valid":{},"wr_vbank_id":{}}}"#,
            rtl_clk(),
            rob_id,
            access.rd0_valid,
            access.rd0_vbank_id,
            access.rd1_valid,
            access.rd1_vbank_id,
            access.wr_valid,
            access.wr_vbank_id
        ));

        let mut read_refs = BTreeSet::new();
        for vbank_id in access.read_vbanks() {
            for (logical_id, _) in resolve_current_logical_bank_mapping(vbank_id, &self.bank_cfgs, &self.bank_map) {
                read_refs.insert(BankVersionRef {
                    bank_id: logical_id,
                    version: self.bank_versions.get(&logical_id).copied().unwrap_or(0),
                });
            }
        }
        if let Some(producer) = self.producer_metadata.get_mut(&rob_id) {
            producer.reads = read_refs;
        }
        if access.wr_valid {
            self.record_decoded_writer(rob_id, access.wr_vbank_id as u64);
        }
    }

    fn record_decoded_writer(&mut self, rob_id: u64, wr_vbank_id: u64) {
        let (affected_bank_set, affected_bank_source) =
            resolve_rtl_affected_banks(wr_vbank_id, &self.bank_cfgs, &self.bank_map);
        let logical_mapping = resolve_current_logical_bank_mapping(wr_vbank_id as u32, &self.bank_cfgs, &self.bank_map);
        let Some(producer) = self.producer_metadata.get(&rob_id) else {
            return;
        };
        if producer.instruction_id == 0 {
            return;
        }
        let instruction_id = producer.instruction_id;
        let writer_source = producer.writer_source;

        for (_, pbank_id) in &logical_mapping {
            if let Some((owner, _)) = self.bank_writers[*pbank_id].first_key_value() {
                if *owner != instruction_id {
                    self.protocol_error(
                        rob_id,
                        format!(
                            "overlapping_bank_write_ownership pbank_id={} owner={} contender={}",
                            pbank_id, owner, instruction_id
                        ),
                    );
                    return;
                }
            }
        }

        let producer = self.producer_metadata.get_mut(&rob_id).expect("producer checked above");
        producer.target_vbank_id = Some(wr_vbank_id as u32);
        producer.affected_bank_set = affected_bank_set;
        producer.expected_logical_banks = logical_mapping.iter().map(|(logical, _)| *logical).collect();
        producer.logical_to_physical = logical_mapping.into_iter().collect();
        producer.affected_bank_source = affected_bank_source;
        for &bank_id in &producer.affected_bank_set {
            self.bank_writers[bank_id].insert(instruction_id, writer_source);
        }
    }

    fn apply_mset(&mut self, xs1: u64, xs2: u64) {
        let bank_id = rs1_b0(xs1);
        if bank_id >= BANK_NUM as u64 {
            return;
        }

        let (_rows, cols, alloc) = xs2_mset(xs2);
        let vbank_id = bank_id as u32;
        let bank_idx = bank_id as usize;
        self.bank_map.delete_vbank(vbank_id);

        if alloc == 1 {
            let groups = cols.max(1).min(BANK_NUM as u64);
            for group in 0..groups {
                if let Some(pbank_id) = self.bank_map.first_free_pbank() {
                    self.bank_map.bind_group(pbank_id, vbank_id, group as u32);
                }
            }
            self.bank_cfgs[bank_idx] = RtlBankConfig { allocated: true, cols };
        } else {
            self.bank_cfgs[bank_idx] = RtlBankConfig {
                allocated: false,
                cols: 0,
            };
        }
    }

    fn record_write_request(&mut self, pbank_id: u32) {
        let bank_id = pbank_id as usize;
        if bank_id >= BANK_NUM {
            return;
        }

        self.write_request_counts[bank_id] = self.write_request_counts[bank_id].wrapping_add(1);
    }

    /// Attribute an accepted private-SRAM write to its architectural writer.
    /// GlobalROB's WAW hazard prevents two issued writers from targeting the
    /// same vbank simultaneously, so the MTrace vbank is a unique lookup key.
    fn record_actual_write(&mut self, vbank_id: u32, pbank_id: u32, group_id: u32, row_addr: u32) {
        self.record_write_request(pbank_id);

        // The RTL boot sequencer clears private SRAM before the first software
        // NPU operation (all of those synthetic itrace records have pc=0).
        // They are initialization, not architectural writes to attribute.
        if self.next_op_id == 0 {
            return;
        }

        let candidates: Vec<u64> = self
            .producer_metadata
            .iter()
            .filter_map(|(rob_id, producer)| (producer.target_vbank_id == Some(vbank_id)).then_some(*rob_id))
            .collect();
        if candidates.len() == 1 {
            if let Some(producer) = self.producer_metadata.get_mut(&candidates[0]) {
                let logical_id = logical_bank_id(vbank_id, group_id);
                producer.actual_logical_banks.insert(logical_id);
                match producer.logical_to_physical.insert(logical_id, pbank_id as usize) {
                    Some(previous) if previous != pbank_id as usize => {
                        self.protocol_error(candidates[0], "logical_bank_remapped_during_instruction");
                    }
                    _ => {}
                }
            }
            return;
        }

        // Commands without a private-Bank comparison boundary (for example
        // Gemmini CISC loops) may still generate implementation-local SRAM
        // traffic. Such traffic does not identify an ambiguous compared
        // write, so retain the global write counter without reporting a
        // DiffTest failure. A completing producer remains in metadata until
        // the eval boundary, so its same-eval tail writes do not take this
        // path.
        self.protocol_error(
            candidates.first().copied().unwrap_or(u64::MAX),
            format!(
                "{} vbank_id={} pbank_id={} row_addr={} candidates={candidates:?}",
                if candidates.is_empty() {
                    "unknown_producer_for_visible_write"
                } else {
                    "ambiguous_producer_for_visible_write"
                },
                vbank_id,
                pbank_id,
                row_addr
            ),
        );
    }

    fn record_write_dispatch(&mut self, rob_id: u64) {
        let Some(producer) = self.producer_metadata.get_mut(&rob_id) else {
            self.protocol_error(rob_id, "write_dispatch_for_unknown_producer");
            return;
        };
        producer.explicit_protocol = true;
        producer.outstanding_writes = producer.outstanding_writes.wrapping_add(1);
    }

    fn record_explicit_visible_write(&mut self, rob_id: u64, vbank_id: u32, pbank_id: u32, group_id: u32) {
        let Some(producer) = self.producer_metadata.get(&rob_id) else {
            self.protocol_error(rob_id, "visible_write_for_unknown_producer");
            return;
        };
        let instruction_id = producer.instruction_id;
        let target_vbank_id = producer.target_vbank_id;
        let outstanding_writes = producer.outstanding_writes;
        let write_end = producer.write_end;
        if instruction_id == 0 {
            if outstanding_writes == 0 {
                self.protocol_error(rob_id, "boot_visible_write_without_dispatch");
                return;
            }
            let producer = self.producer_metadata.get_mut(&rob_id).expect("producer checked above");
            producer.outstanding_writes -= 1;
            self.completed_this_eval.push(rob_id);
            RTL_BANK_HASH_STATE.with(|state| state.set(state.get() | 2));
            return;
        }
        if pbank_id as usize >= BANK_NUM {
            self.protocol_error(rob_id, format!("visible_write_invalid_pbank pbank={pbank_id}"));
            return;
        }
        if target_vbank_id != Some(vbank_id) {
            self.protocol_error(
                rob_id,
                format!("visible_write_wrong_vbank expected={target_vbank_id:?} actual={vbank_id}"),
            );
            return;
        }
        if !self.bank_writers[pbank_id as usize].contains_key(&instruction_id) {
            self.protocol_error(rob_id, format!("visible_write_without_bank_ownership pbank={pbank_id}"));
            return;
        }
        if write_end && outstanding_writes == 0 {
            self.protocol_error(rob_id, "late_write_after_stable_boundary");
            return;
        }
        if outstanding_writes == 0 {
            self.protocol_error(rob_id, "visible_write_without_dispatch");
            return;
        }
        let producer = self.producer_metadata.get_mut(&rob_id).expect("producer checked above");
        producer.outstanding_writes -= 1;
        let logical_id = logical_bank_id(vbank_id, group_id);
        producer.actual_logical_banks.insert(logical_id);
        producer.logical_to_physical.insert(logical_id, pbank_id as usize);
        self.completed_this_eval.push(rob_id);
        RTL_BANK_HASH_STATE.with(|state| state.set(state.get() | 2));
    }

    fn record_write_end(&mut self, rob_id: u64) {
        let Some(producer) = self.producer_metadata.get_mut(&rob_id) else {
            if self.retired_boot_rob_ids.contains(&rob_id) {
                return;
            }
            self.protocol_error(rob_id, "write_end_for_unknown_producer");
            return;
        };
        if producer.write_end {
            self.protocol_error(rob_id, "duplicate_write_end");
            return;
        }
        producer.explicit_protocol = true;
        producer.write_end = true;
        self.completed_this_eval.push(rob_id);
        RTL_BANK_HASH_STATE.with(|state| state.set(state.get() | 2));
    }

    fn record_cancel(&mut self, rob_id: u64) {
        let Some(producer) = self.producer_metadata.get_mut(&rob_id) else {
            self.protocol_error(rob_id, "cancel_for_unknown_producer");
            return;
        };
        producer.cancelled = true;
        producer.write_end = true;
        self.completed_this_eval.push(rob_id);
        RTL_BANK_HASH_STATE.with(|state| state.set(state.get() | 2));
    }

    fn check_bank_targets(&mut self, producer: &ProducerMeta) {
        if producer.instruction_id == 0 || producer.cancelled {
            return;
        }

        self.bank_target_checks = self.bank_target_checks.wrapping_add(1);
        let missing_banks: BTreeSet<_> = producer
            .expected_logical_banks
            .difference(&producer.actual_logical_banks)
            .copied()
            .collect();
        let unexpected_banks: BTreeSet<_> = producer
            .actual_logical_banks
            .difference(&producer.expected_logical_banks)
            .copied()
            .collect();
        let decoded_target_missing = producer.bank_enable & 2 != 0 && producer.target_vbank_id.is_none();
        let expected_mapping_empty = producer.target_vbank_id.is_some() && producer.expected_logical_banks.is_empty();
        // The decoded ROB set is dependency/hazard metadata, not the
        // architectural T_exp. Config/allocation instructions may declare a
        // writer without producing an SPM-visible write. BEMU supplies T_exp
        // at the stable boundary; locally we only reject undecodable or
        // out-of-declaration visible writes while the decoded mapping is
        // still available. If the mapping has already been released, the
        // actual write set remains authoritative and is checked against BEMU
        // by the boundary comparator.
        let matches = !decoded_target_missing && (expected_mapping_empty || unexpected_banks.is_empty());
        if !matches {
            self.bank_target_mismatches = self.bank_target_mismatches.wrapping_add(1);
        }

        let expected_banks: Vec<_> = producer.expected_logical_banks.iter().copied().collect();
        let actual_banks: Vec<_> = producer.actual_logical_banks.iter().copied().collect();
        let missing_banks: Vec<_> = missing_banks.into_iter().collect();
        let unexpected_banks: Vec<_> = unexpected_banks.into_iter().collect();
        let target_vbank_id = producer
            .target_vbank_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "null".to_string());

        write_trace(&format!(
            r#"{{"type":"bank_target_check","result":"{}","clk":{},"op_id":{},"rob_id":{},"target_vbank_id":{},"funct7":{},"pc":"0x{:016x}","decoded_target_missing":{},"expected_mapping_empty":{},"expected_banks":{:?},"actual_banks":{:?},"missing_banks":{:?},"unexpected_banks":{:?}}}"#,
            if matches { "PASS" } else { "MISMATCH" },
            rtl_clk(),
            producer.instruction_id,
            producer.rob_id,
            target_vbank_id,
            producer.funct7,
            producer.pc,
            decoded_target_missing,
            expected_mapping_empty,
            expected_banks,
            actual_banks,
            missing_banks,
            unexpected_banks
        ));
        if !matches {
            record_rtl_difftest_failure();
        }
    }

    /// Record an architectural completion, but keep the producer alive until
    /// the enclosing Verilator eval returns. Memory-write DPI callbacks from
    /// the same clock edge are not ordered with respect to the completion DPI
    /// callback, so retiring the producer here can lose its final Bank write.
    fn record_completion(&mut self, rob_id: u64, funct7: u32, op_type: &str, cycle: u64, pc: u64) {
        if pc == 0 && !self.producer_metadata.contains_key(&rob_id) && self.retired_boot_rob_ids.contains(&rob_id) {
            return;
        }
        let producer = self.producer_metadata.entry(rob_id).or_insert_with(|| ProducerMeta {
            rob_id,
            instruction_id: rob_id,
            semantic_seq: rob_id,
            funct7,
            op_type: op_type.to_string(),
            pc,
            bank_enable: 0,
            target_vbank_id: None,
            affected_bank_set: BTreeSet::new(),
            expected_logical_banks: BTreeSet::new(),
            actual_logical_banks: BTreeSet::new(),
            logical_to_physical: BTreeMap::new(),
            reads: BTreeSet::new(),
            affected_bank_source: AffectedBankSource::SoftwareMirrorFallback,
            alloc_cycle: cycle,
            complete_cycle: None,
            outstanding_writes: 0,
            write_end: false,
            cancelled: false,
            explicit_protocol: false,
            writer_source: WriterSource::WritebackUnit,
        });
        if producer.complete_cycle.is_some() {
            self.protocol_error(rob_id, "duplicate_architectural_completion");
            return;
        }
        producer.complete_cycle = Some(cycle);
        if !producer.explicit_protocol || producer.instruction_id == 0 {
            producer.write_end = true;
        }
        if !self.completed_this_eval.contains(&rob_id) {
            self.completed_this_eval.push(rob_id);
        }
    }

    /// Finalize every completion observed in the preceding Verilator eval.
    /// GlobalROB prevents a younger same-vbank writer from issuing in the
    /// completion cycle, so this is the unique architectural snapshot point:
    /// all writes from the completed producer are visible and no successor can
    /// have modified the same physical Banks yet.
    fn finalize_completed_instructions(&mut self) -> Vec<StableInstructionTask> {
        let mut stable_tasks = Vec::new();
        let ready: BTreeSet<_> = std::mem::take(&mut self.completed_this_eval).into_iter().collect();
        for rob_id in ready {
            let is_stable = self
                .producer_metadata
                .get(&rob_id)
                .is_some_and(|producer| producer.write_end && producer.outstanding_writes == 0);
            if !is_stable {
                continue;
            }
            let Some(producer) = self.producer_metadata.remove(&rob_id) else {
                continue;
            };
            if producer.instruction_id == 0 {
                self.retired_boot_rob_ids.insert(rob_id);
                for &bank_id in &producer.affected_bank_set {
                    self.bank_writers[bank_id].remove(&producer.instruction_id);
                }
                continue;
            }
            self.check_bank_targets(&producer);

            let complete_cycle = producer.complete_cycle.unwrap_or_else(rtl_clk);
            let mut hashes = Vec::new();
            let mut writes = Vec::new();
            if !producer.cancelled {
                for &logical_id in &producer.actual_logical_banks {
                    let Some(&pbank_id) = producer.logical_to_physical.get(&logical_id) else {
                        self.protocol_error(rob_id, format!("missing_physical_mapping logical_bank={logical_id}"));
                        continue;
                    };
                    maybe_inject_spm_fault(producer.semantic_seq, logical_id, pbank_id as u32);
                    let Some(hash) = hash_rtl_private_bank(pbank_id as u32) else {
                        self.protocol_error(
                            rob_id,
                            format!("rtl_private_bank_backdoor_unavailable pbank={pbank_id}"),
                        );
                        continue;
                    };
                    let version = self.bank_versions.entry(logical_id).or_insert(0);
                    *version = version.wrapping_add(1);
                    writes.push(BankDigest {
                        bank_id: logical_id,
                        version: *version,
                        hash,
                    });
                    self.task_count = self.task_count.wrapping_add(1);
                    let task = PendingHashTask {
                        task_id: self.task_count,
                        instruction_id: producer.instruction_id,
                        bank_id: pbank_id,
                        funct7: producer.funct7,
                        op_type: producer.op_type.clone(),
                        cycle: complete_cycle,
                        pc: producer.pc,
                        bank_enable: producer.bank_enable,
                        alloc_cycle: producer.alloc_cycle,
                        complete_cycle,
                        stable_cycle: None,
                        observed_write_count: self.write_request_counts[pbank_id],
                        writer_source: producer.writer_source,
                        affected_bank_source: producer.affected_bank_source,
                    };
                    let snapshot = BankStabilitySnapshot {
                        pending_same_bank_writes: 0,
                    };
                    write_bank_hash_stability_event("stable", &task, snapshot);
                    hashes.push(StableHashTask {
                        instruction_id: producer.instruction_id,
                        semantic_seq: producer.semantic_seq,
                        bank_id: logical_id,
                        bank_version: *version,
                        hash,
                        funct7: producer.funct7,
                        op_type: producer.op_type.clone(),
                        cycle: complete_cycle,
                        pc: producer.pc,
                    });
                }
            }

            // Snapshot capture above occurs while this producer still owns
            // every affected physical Bank. Only now may a successor write.
            for &bank_id in &producer.affected_bank_set {
                self.bank_writers[bank_id].remove(&producer.instruction_id);
            }

            stable_tasks.push(StableInstructionTask {
                boundary: InstructionBankBoundaryPacket {
                    record_type: "instruction_bank_boundary",
                    source: BankHashSource::Rtl,
                    instruction_id: producer.instruction_id,
                    semantic_seq: producer.semantic_seq,
                    funct7: producer.funct7,
                    pc: producer.pc,
                    expected_banks: producer.expected_logical_banks.iter().copied().collect(),
                    actual_banks: producer.actual_logical_banks.iter().copied().collect(),
                    reads: producer.reads.iter().copied().collect(),
                    writes,
                    cycle: complete_cycle,
                    cancelled: producer.cancelled,
                },
                hashes,
            });
        }
        stable_tasks
    }
}

fn get_verilator_top() -> &'static AtomicPtr<VerilatorTop> {
    VERILATOR_TOP.get_or_init(|| AtomicPtr::new(ptr::null_mut()))
}

pub fn set_verilator_top(top: *mut VerilatorTop) {
    get_verilator_top().store(top, Ordering::SeqCst);
}

fn hash_rtl_private_bank(bank_id: u32) -> Option<u64> {
    let top = get_verilator_top().load(Ordering::SeqCst);
    if top.is_null() {
        return None;
    }
    let mut hash = 0_u64;
    let ok = unsafe { verilator_hash_private_bank(top, bank_id, &mut hash) };
    ok.then_some(hash)
}

fn rs1_b0(xs1: u64) -> u64 {
    xs1 & 0x3ff
}

fn xs2_mset(xs2: u64) -> (u64, u64, u64) {
    let rows = xs2 & 0x1f;
    let cols = (xs2 >> 5) & 0x1f;
    let alloc = (xs2 >> 10) & 1;
    (rows, cols, alloc)
}

const fn logical_bank_id(vbank_id: u32, group_id: u32) -> u32 {
    vbank_id * BANK_NUM as u32 + group_id
}

fn add_resolved_bank(out: &mut BTreeSet<usize>, bank_map: &RtlBankMap, vbank: u64, group: u64) {
    if vbank < BANK_NUM as u64 {
        if let Some(pbank_id) = bank_map.resolve_group(vbank as u32, group as u32) {
            out.insert(pbank_id);
        }
    }
}

fn add_vbank_groups(out: &mut BTreeSet<usize>, cfgs: &[RtlBankConfig; BANK_NUM], bank_map: &RtlBankMap, vbank: u64) {
    if vbank >= cfgs.len() as u64 {
        return;
    }

    if !cfgs[vbank as usize].allocated {
        return;
    }

    let groups = cfgs[vbank as usize].cols.max(1).min(BANK_NUM as u64);
    for group in 0..groups {
        add_resolved_bank(out, bank_map, vbank, group);
    }
}

fn read_rtl_private_mapping(vbank: u64) -> Option<BTreeSet<usize>> {
    let top = get_verilator_top().load(Ordering::SeqCst);
    if top.is_null() || vbank >= BANK_NUM as u64 {
        return None;
    }

    let mut pbank_mask = 0u32;
    let ok = unsafe { verilator_resolve_private_bank_mask(top, vbank as u32, &mut pbank_mask as *mut u32) };
    ok.then(|| {
        (0..BANK_NUM)
            .filter(|bank_id| pbank_mask & (1u32 << bank_id) != 0)
            .collect()
    })
}

#[derive(Clone, Copy, Debug)]
struct DecodedBankAccess {
    rd0_valid: bool,
    rd0_vbank_id: u32,
    rd1_valid: bool,
    rd1_vbank_id: u32,
    wr_valid: bool,
    wr_vbank_id: u32,
}

impl DecodedBankAccess {
    fn read_vbanks(self) -> impl Iterator<Item = u32> {
        [
            self.rd0_valid.then_some(self.rd0_vbank_id),
            self.rd1_valid.then_some(self.rd1_vbank_id),
        ]
        .into_iter()
        .flatten()
    }
}

fn read_rtl_decoded_bank_access(rob_id: u32) -> Option<DecodedBankAccess> {
    let top = get_verilator_top().load(Ordering::SeqCst);
    if top.is_null() {
        return None;
    }

    let mut rd0_valid = false;
    let mut rd0_vbank_id = 0u32;
    let mut rd1_valid = false;
    let mut rd1_vbank_id = 0u32;
    let mut wr_valid = false;
    let mut wr_vbank_id = 0u32;
    let ok = unsafe {
        verilator_read_rob_bank_access(
            top,
            rob_id,
            &mut rd0_valid,
            &mut rd0_vbank_id,
            &mut rd1_valid,
            &mut rd1_vbank_id,
            &mut wr_valid,
            &mut wr_vbank_id,
        )
    };
    ok.then_some(DecodedBankAccess {
        rd0_valid,
        rd0_vbank_id,
        rd1_valid,
        rd1_vbank_id,
        wr_valid,
        wr_vbank_id,
    })
}

fn resolve_logical_bank_mapping(
    vbank_id: u32,
    cfgs: &[RtlBankConfig; BANK_NUM],
    bank_map: &RtlBankMap,
) -> BTreeMap<u32, usize> {
    let mut out = BTreeMap::new();
    if vbank_id as usize >= BANK_NUM || !cfgs[vbank_id as usize].allocated {
        return out;
    }
    let groups = cfgs[vbank_id as usize].cols.max(1).min(BANK_NUM as u64);
    for group_id in 0..groups as u32 {
        if let Some(pbank_id) = bank_map.resolve_group(vbank_id, group_id) {
            out.insert(logical_bank_id(vbank_id, group_id), pbank_id);
        }
    }
    out
}

/// Resolve the current logical-to-physical mapping from the RTL whenever a
/// Verilated top is available. The software mirror can lag allocation-table
/// lifetime transitions, while the decoded ROB access and visible writes are
/// defined by the live RTL mapping.
fn resolve_current_logical_bank_mapping(
    vbank_id: u32,
    cfgs: &[RtlBankConfig; BANK_NUM],
    bank_map: &RtlBankMap,
) -> BTreeMap<u32, usize> {
    if let Some(pbanks) = read_rtl_private_mapping(u64::from(vbank_id)) {
        return pbanks
            .into_iter()
            .enumerate()
            .map(|(group_id, pbank_id)| (logical_bank_id(vbank_id, group_id as u32), pbank_id))
            .collect();
    }
    resolve_logical_bank_mapping(vbank_id, cfgs, bank_map)
}

/// Resolve affected physical Banks from the RTL backend's real mapping table.
/// The software mirror exists only so pure Rust unit tests can run without a
/// Verilated top and is never preferred during simulation.
fn resolve_rtl_affected_banks(
    write_vbank: u64,
    cfgs: &[RtlBankConfig; BANK_NUM],
    bank_map: &RtlBankMap,
) -> (BTreeSet<usize>, AffectedBankSource) {
    if let Some(out) = read_rtl_private_mapping(write_vbank) {
        return (out, AffectedBankSource::RtlPrivateMapping);
    }

    let mut fallback = BTreeSet::new();
    add_vbank_groups(&mut fallback, cfgs, bank_map, write_vbank);
    (fallback, AffectedBankSource::SoftwareMirrorFallback)
}

#[derive(Clone, Debug, Default)]
pub struct TraceConfig {
    pub itrace: bool,
    pub mtrace: bool,
    pub pmctrace: bool,
    pub ctrace: bool,
    pub banktrace: bool,
    /// Enable the Bank-hash monitor used by DiffTest without also enabling
    /// the high-volume human-readable Bank trace.
    pub bank_hash: bool,
    pub spm_fault: Option<SpmFaultConfig>,
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

pub fn init_trace(log_dir: &Path, config: TraceConfig) -> io::Result<()> {
    std::fs::create_dir_all(log_dir)?;
    RTL_BANK_HASH_STATE.with(|state| state.set(u8::from(config.banktrace || config.bank_hash)));
    let log_path = log_dir.join("bdb.ndjson");
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_path)?;

    TRACE_FILE.with(|slot| *slot.borrow_mut() = Some(file));
    ENABLE_ITRACE.with(|enabled| enabled.set(config.itrace));
    ENABLE_MTRACE.with(|enabled| enabled.set(config.mtrace));
    ENABLE_PMCTRACE.with(|enabled| enabled.set(config.pmctrace));
    ENABLE_CTRACE.with(|enabled| enabled.set(config.ctrace));
    ENABLE_BANKTRACE.with(|enabled| enabled.set(config.banktrace));
    RTL_SPM_FAULT_STATE.with(|state| state.borrow_mut().reset(config.spm_fault));
    if config.banktrace || config.bank_hash {
        init_rtl_bank_hash_trace(
            &log_dir.join("rtl_bank_hash.ndjson"),
            &log_dir.join("btrace_log.ndjson"),
        )?;
    }
    Ok(())
}

fn init_rtl_bank_hash_trace(log_path: &Path, btrace_log_path: &Path) -> io::Result<()> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_path)?;

    RTL_BANK_HASH_FILE.with(|slot| *slot.borrow_mut() = Some(file));
    let btrace_log = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(btrace_log_path)?;
    RTL_BTRACE_LOG_FILE.with(|slot| *slot.borrow_mut() = Some(btrace_log));
    RTL_BANK_HASH_STATE.with(|state| state.set(1));
    with_rtl_bank_stability_monitor(BankStabilityMonitor::reset);
    RTL_BTRACE_STATE.with(|state| state.borrow_mut().reset());
    Ok(())
}

pub fn set_rtl_clk(clk: u64) {
    RTL_CLK.with(|value| value.set(clk));
}

pub fn rtl_clk() -> u64 {
    RTL_CLK.with(Cell::get)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RtlBankHashEvalOutcome {
    pub events: u32,
    pub failure: bool,
    pub pending: bool,
}

/// Complete the Bank Hash work triggered by DPI callbacks in the preceding
/// Verilator eval. Completion is finalized here, after every write callback
/// from the same eval has been observed and before the next RTL edge can issue
/// a younger same-vbank writer.
pub fn finish_rtl_bank_hash_eval() -> RtlBankHashEvalOutcome {
    let initial_state = RTL_BANK_HASH_STATE.with(Cell::get);
    if initial_state & 6 == 0 {
        return RtlBankHashEvalOutcome::default();
    }

    let events = if initial_state & 2 != 0 {
        let stable_tasks = with_rtl_bank_stability_monitor(BankStabilityMonitor::finalize_completed_instructions);
        emit_stable_rtl_bank_hash_tasks(stable_tasks)
    } else {
        0
    };

    let failure = RTL_BANK_HASH_STATE.with(|state| {
        let failure = state.get() & 4 != 0;
        state.set(initial_state & 1);
        failure
    });
    RtlBankHashEvalOutcome {
        events,
        failure,
        pending: false,
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RtlBankDifftestStatus {
    pub in_flight_operations: usize,
    pub in_flight_bank_writers: usize,
    pub pending_stable_boundaries: usize,
    pub bank_target_checks: u64,
    pub bank_target_mismatches: u64,
    pub bank_write_attribution_errors: u64,
    pub spm_fault_injected: bool,
}

impl RtlBankDifftestStatus {
    pub fn is_drained(self) -> bool {
        self.in_flight_operations == 0 && self.in_flight_bank_writers == 0 && self.pending_stable_boundaries == 0
    }

    pub fn bank_targets_are_clean(self) -> bool {
        self.bank_target_mismatches == 0 && self.bank_write_attribution_errors == 0
    }
}

pub fn rtl_bank_difftest_status() -> RtlBankDifftestStatus {
    with_rtl_bank_stability_monitor(|monitor| RtlBankDifftestStatus {
        in_flight_operations: monitor.producer_metadata.len(),
        in_flight_bank_writers: monitor.bank_writers.iter().map(BTreeMap::len).sum(),
        pending_stable_boundaries: monitor.completed_this_eval.len(),
        bank_target_checks: monitor.bank_target_checks,
        bank_target_mismatches: monitor.bank_target_mismatches,
        bank_write_attribution_errors: monitor.bank_write_attribution_errors,
        spm_fault_injected: RTL_SPM_FAULT_STATE.with(|state| state.borrow().injected),
    })
}

fn write_trace(json: &str) {
    TRACE_FILE.with(|slot| {
        if let Some(file) = slot.borrow_mut().as_mut() {
            writeln!(file, "{}", json).ok();
            file.flush().ok();
        }
    });
}

fn write_bank_hash_stability_event(event: &str, task: &PendingHashTask, snapshot: BankStabilitySnapshot) {
    let json = format!(
        r#"{{"type":"bank_hash_stability","clk":{},"event":"{}","task_id":{},"source":"RTL","op_id":{},"bank_id":{},"funct7":{},"op_type":"{}","writer_source":"{}","affected_bank_source":"{}","pc":"0x{:016x}","bank_enable":{},"alloc_cycle":{},"complete_cycle":{},"stable_cycle":{},"observed_write_count":{},"pending_same_bank_writes":{},"strategy":"complete_eval_boundary"}}"#,
        task.cycle,
        event,
        task.task_id,
        task.instruction_id,
        task.bank_id,
        task.funct7,
        task.op_type,
        task.writer_source.as_str(),
        task.affected_bank_source.as_str(),
        task.pc,
        task.bank_enable,
        task.alloc_cycle,
        task.complete_cycle,
        task.stable_cycle.unwrap_or(0),
        task.observed_write_count,
        snapshot.pending_same_bank_writes
    );
    write_trace(&json);
}

fn write_rtl_btrace_log(line: &str) {
    RTL_BTRACE_LOG_FILE.with(|slot| {
        if let Some(file) = slot.borrow_mut().as_mut() {
            file.write_all(line.as_bytes()).ok();
            file.flush().ok();
        }
    });
}

fn write_rtl_bank_hash_packet(task: &StableHashTask, hash: u64) {
    let mut packet = BankHashPacket::new(
        BankHashSource::Rtl,
        BankHashPacketId::InstructionId(task.instruction_id),
        task.bank_id,
        &task.op_type,
        hash,
        BankHashTime::Cycle(task.cycle),
    );
    packet.version = task.bank_version;
    let raw_line = RTL_BTRACE_STATE.with(|state| state.borrow_mut().next_raw_line());
    if let Ok(line) = packet.to_ndjson() {
        RTL_BANK_HASH_FILE.with(|slot| {
            if let Some(file) = slot.borrow_mut().as_mut() {
                file.write_all(line.as_bytes()).ok();
                file.flush().ok();
            }
        });
    }

    let btrace_packet = CanonicalBankHashPacket::new(
        BankHashSource::Rtl,
        task.instruction_id,
        Some(task.semantic_seq),
        task.bank_id,
        task.funct7,
        &task.op_type,
        BankHashEventClass::BankDataWrite,
        hash,
        BankHashTime::Cycle(task.cycle),
        Some(task.pc),
        format!("rtl_bank_hash.ndjson:{raw_line}"),
        raw_line,
    )
    .with_bank_version(task.bank_version);
    if let Ok(line) = btrace_packet.to_ndjson() {
        write_rtl_btrace_log(&line);
    }
}

fn emit_stable_rtl_bank_hash_tasks(stable_tasks: Vec<StableInstructionTask>) -> u32 {
    let mut events = 0_u32;
    for task in stable_tasks {
        for hash_task in &task.hashes {
            write_rtl_bank_hash_packet(hash_task, hash_task.hash);
            events = events.saturating_add(1);
        }
        submit_runtime_bank_boundary(task.boundary);
        events = events.saturating_add(1);
    }
    events
}

// Instruction trace
pub fn itrace(event: ITraceEvent) {
    let bank_hash_enabled = rtl_bank_hash_enabled();
    if bank_hash_enabled && event.is_issue == 2 {
        with_rtl_bank_stability_monitor(|monitor| monitor.record_allocation(&event, rtl_clk()));
    }

    if bank_hash_enabled && event.is_issue == 1 {
        with_rtl_bank_stability_monitor(|monitor| monitor.record_issue(&event));
    }

    if bank_hash_enabled && event.is_issue == 0 {
        let op_type = format!("funct7_{}", event.funct);
        with_rtl_bank_stability_monitor(|monitor| {
            monitor.record_completion(event.rob_id as u64, event.funct, &op_type, rtl_clk(), event.pc)
        });
        RTL_BANK_HASH_STATE.with(|state| state.set(state.get() | 2));
    }

    if !ENABLE_ITRACE.with(Cell::get) {
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

    if !ENABLE_MTRACE.with(Cell::get) {
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

pub fn bank_write_dispatch(rob_id: u32) {
    if rtl_bank_hash_enabled() {
        with_rtl_bank_stability_monitor(|monitor| monitor.record_write_dispatch(rob_id as u64));
    }
}

pub fn bank_write_visible(rob_id: u32, vbank_id: u32, pbank_id: u32, group_id: u32) {
    if rtl_bank_hash_enabled() {
        with_rtl_bank_stability_monitor(|monitor| {
            monitor.record_explicit_visible_write(rob_id as u64, vbank_id, pbank_id, group_id)
        });
    }
}

pub fn bank_write_end(rob_id: u32) {
    if rtl_bank_hash_enabled() {
        with_rtl_bank_stability_monitor(|monitor| monitor.record_write_end(rob_id as u64));
    }
}

pub fn bank_instruction_cancel(rob_id: u32) {
    if rtl_bank_hash_enabled() {
        with_rtl_bank_stability_monitor(|monitor| monitor.record_cancel(rob_id as u64));
    }
}

pub fn banktrace(event: BankTraceEvent) {
    if rtl_bank_hash_enabled() && event.event == "backdoor_write" && event.is_shared == 0 {
        with_rtl_bank_stability_monitor(|monitor| {
            monitor.record_actual_write(event.vbank_id, event.pbank_id, event.group_id, event.addr)
        });
    }

    if !ENABLE_BANKTRACE.with(Cell::get) {
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
    if !ENABLE_PMCTRACE.with(Cell::get) {
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
    if !ENABLE_PMCTRACE.with(Cell::get) {
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
    if !ENABLE_CTRACE.with(Cell::get) {
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

// Historical tests for the completion-only monitor are retained as design
// history, but the protocol monitor below supersedes their assumptions.
#[cfg(any())]
mod tests {
    use super::*;

    #[test]
    fn rtl_eval_outcome_fast_path_and_failure_latch_need_no_atomics() {
        RTL_BANK_HASH_STATE.with(|state| state.set(1));
        assert_eq!(finish_rtl_bank_hash_eval(), RtlBankHashEvalOutcome::default());

        record_rtl_difftest_failure();
        assert_eq!(
            finish_rtl_bank_hash_eval(),
            RtlBankHashEvalOutcome {
                events: 0,
                failure: true,
                pending: false,
            }
        );
        assert_eq!(finish_rtl_bank_hash_eval(), RtlBankHashEvalOutcome::default());
    }

    #[test]
    fn rtl_comparable_seq_is_shared_by_all_banks_of_one_completed_writer() {
        RTL_BTRACE_STATE.with(|state| state.borrow_mut().reset());
        let mut monitor = BankStabilityMonitor::new();
        let mset_cols4_alloc = (4 << 5) | (1 << 10);
        monitor.apply_mset(0, mset_cols4_alloc);
        monitor.apply_mset(4, mset_cols4_alloc);

        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 1,
                domain_id: 1,
                funct: 33,
                pc: 0x8000_0634,
                rs1: 0,
                rs2: 0,
                bank_enable: 2,
            },
            10,
        );
        monitor.record_decoded_writer(1, 0);
        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 2,
                domain_id: 1,
                funct: 33,
                pc: 0x8000_0638,
                rs1: 4,
                rs2: 0,
                bank_enable: 2,
            },
            11,
        );
        monitor.record_decoded_writer(2, 4);

        for bank_id in 4..8 {
            monitor.record_actual_write(4, bank_id, 0);
        }
        monitor.record_completion(2, 33, "funct7_33", 20, 0x8000_0638);
        for bank_id in 0..4 {
            monitor.record_actual_write(0, bank_id, 0);
        }
        monitor.record_completion(1, 33, "funct7_33", 21, 0x8000_0634);
        let tasks = monitor.finalize_completed_instructions();

        let mut task_seq_by_bank = BTreeMap::new();
        for task in &tasks {
            task_seq_by_bank.insert(task.bank_id, task.comparable_seq);
        }

        for bank_id in 0..4 {
            assert_eq!(task_seq_by_bank.get(&bank_id), Some(&Some(2)));
        }
        for bank_id in 4..8 {
            assert_eq!(task_seq_by_bank.get(&bank_id), Some(&Some(1)));
        }
    }

    #[test]
    fn rtl_gemmini_preload_does_not_require_a_declared_output_bank_write() {
        RTL_BTRACE_STATE.with(|state| state.borrow_mut().reset());
        let mut monitor = BankStabilityMonitor::new();
        let mset_cols1_alloc = (1 << 5) | (1 << 10);
        let wr_vbank = 4;
        let rs1 = wr_vbank << 20;

        monitor.apply_mset(wr_vbank, mset_cols1_alloc);
        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 1,
                domain_id: 3,
                funct: 53,
                pc: 0x8000_1000,
                rs1,
                rs2: 0,
                bank_enable: 3,
            },
            10,
        );
        monitor.record_decoded_writer(1, wr_vbank);
        monitor.record_actual_write(wr_vbank as u32, 0, 0);
        monitor.record_completion(1, 53, "funct7_53", 20, 0x8000_1000);
        let tasks = monitor.finalize_completed_instructions();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].bank_id, 0);
        assert_eq!(tasks[0].comparable_seq, Some(1));
        assert_eq!(monitor.bank_target_checks, 0);
        assert_eq!(monitor.bank_target_mismatches, 0);
    }

    #[test]
    fn rtl_gemmini_instruction_without_sram_write_emits_no_boundary() {
        RTL_BTRACE_STATE.with(|state| state.borrow_mut().reset());
        let mut monitor = BankStabilityMonitor::new();
        let mset_cols1_alloc = (1 << 5) | (1 << 10);
        let wr_vbank = 4;
        let rs1 = wr_vbank << 20;

        monitor.apply_mset(wr_vbank, mset_cols1_alloc);
        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 1,
                domain_id: 0,
                funct: 2,
                pc: 0x8000_0ff0,
                rs1: 0,
                rs2: 1 << 4,
                bank_enable: 0,
            },
            9,
        );
        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 2,
                domain_id: 3,
                funct: 53,
                pc: 0x8000_1000,
                rs1,
                rs2: 0,
                bank_enable: 3,
            },
            10,
        );
        monitor.record_decoded_writer(2, wr_vbank);
        monitor.record_completion(2, 53, "funct7_53", 20, 0x8000_1000);
        let tasks = monitor.finalize_completed_instructions();

        assert!(tasks.is_empty());
        assert_eq!(monitor.bank_target_checks, 0);
        assert_eq!(monitor.bank_target_mismatches, 0);
    }

    #[test]
    fn overlapping_writer_at_complete_boundary_is_a_protocol_error() {
        RTL_BTRACE_STATE.with(|state| state.borrow_mut().reset());
        let mut monitor = BankStabilityMonitor::new();
        monitor.apply_mset(0, (1 << 5) | (1 << 10));

        for rob_id in [10, 11] {
            monitor.record_allocation(
                &ITraceEvent {
                    is_issue: 2,
                    rob_id,
                    domain_id: 1,
                    funct: 33,
                    pc: 0x8000_1000 + u64::from(rob_id),
                    rs1: 0,
                    rs2: 0,
                    bank_enable: 2,
                },
                u64::from(rob_id),
            );
            monitor.record_decoded_writer(u64::from(rob_id), 0);
        }

        assert_eq!(monitor.bank_writers[0].len(), 2);
        monitor
            .producer_metadata
            .get_mut(&10)
            .unwrap()
            .actual_bank_set
            .insert(0);
        monitor.record_write_request(0);
        monitor.record_completion(10, 33, "funct7_33", 20, 0x8000_100a);
        let tasks = monitor.finalize_completed_instructions();

        assert!(tasks.is_empty());
        assert_eq!(monitor.bank_writers[0].len(), 1);
        assert_eq!(monitor.bank_write_attribution_errors, 1);
        assert_eq!(monitor.bank_versions[0], 0);
    }

    #[test]
    fn same_eval_write_after_completion_is_included_in_boundary() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.apply_mset(0, (1 << 5) | (1 << 10));
        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 1,
                domain_id: 1,
                funct: 33,
                pc: 0x8000_1000,
                rs1: 0,
                rs2: 0,
                bank_enable: 2,
            },
            10,
        );
        monitor.record_decoded_writer(1, 0);

        monitor.record_completion(1, 33, "funct7_33", 20, 0x8000_1000);
        monitor.record_actual_write(0, 0, 7);
        let tasks = monitor.finalize_completed_instructions();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].bank_id, 0);
        assert_eq!(monitor.bank_write_attribution_errors, 0);
        assert!(!monitor.producer_metadata.contains_key(&1));
    }

    #[test]
    fn rtl_boot_initialization_does_not_shift_software_op_ids_or_versions() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 1,
                domain_id: 0,
                funct: 33,
                pc: 0,
                rs1: 0,
                rs2: 0,
                bank_enable: 2,
            },
            1,
        );
        assert_eq!(monitor.next_op_id, 0);
        assert!(monitor.bank_writers.iter().all(BTreeMap::is_empty));
        assert_eq!(monitor.bank_versions, [0; BANK_NUM]);

        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 2,
                domain_id: 0,
                funct: 0,
                pc: 0x8000_1000,
                rs1: 0,
                rs2: 0,
                bank_enable: 0,
            },
            2,
        );
        assert_eq!(monitor.next_op_id, 1);
        assert_eq!(monitor.producer_metadata[&2].instruction_id, 1);
    }

    #[test]
    fn read_only_instruction_never_enters_writer_scoreboard() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.apply_mset(0, (1 << 5) | (1 << 10));
        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 1,
                domain_id: 1,
                funct: 16,
                pc: 0x8000_2000,
                rs1: 0,
                rs2: 0,
                bank_enable: 1,
            },
            10,
        );

        assert!(monitor.producer_metadata[&1].affected_bank_set.is_empty());
        assert!(monitor.bank_writers.iter().all(BTreeMap::is_empty));
    }

    #[test]
    fn decoded_writer_target_does_not_depend_on_funct7_classification() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.apply_mset(0, (1 << 5) | (1 << 10));
        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 1,
                domain_id: 3,
                funct: 127,
                pc: 0x8000_3000,
                rs1: 0,
                rs2: 0,
                bank_enable: 0,
            },
            10,
        );
        monitor.record_decoded_writer(1, 0);
        monitor.record_actual_write(0, 0, 0);
        monitor.record_completion(1, 127, "funct7_127", 20, 0x8000_3000);
        let tasks = monitor.finalize_completed_instructions();

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].bank_id, 0);
    }

    #[test]
    fn mem_domain_writer_without_sram_request_does_not_emit_hash() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.apply_mset(0, (1 << 5) | (1 << 10));
        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 1,
                domain_id: 1,
                funct: 34,
                pc: 0x8000_3000,
                rs1: 0,
                rs2: 0,
                bank_enable: 2,
            },
            10,
        );
        monitor.record_decoded_writer(1, 0);
        monitor.record_completion(1, 34, "funct7_34", 20, 0x8000_3000);
        let tasks = monitor.finalize_completed_instructions();

        assert!(tasks.is_empty());
        assert!(monitor.bank_writers[0].is_empty());
    }

    #[test]
    fn actual_banks_are_attributed_by_unique_in_flight_vbank() {
        let mut monitor = BankStabilityMonitor::new();
        let wr_vbank = 4;
        monitor.apply_mset(wr_vbank, (2 << 5) | (1 << 10));
        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 1,
                domain_id: 3,
                funct: 64,
                pc: 0x8000_4000,
                rs1: wr_vbank << 20,
                rs2: 0,
                bank_enable: 4,
            },
            10,
        );
        monitor.record_decoded_writer(1, wr_vbank);

        monitor.record_actual_write(wr_vbank as u32, 0, 3);
        monitor.record_actual_write(wr_vbank as u32, 1, 7);
        monitor.record_actual_write(wr_vbank as u32, 1, 8);

        assert_eq!(monitor.producer_metadata[&1].actual_bank_set, BTreeSet::from([0, 1]));
        assert_eq!(monitor.bank_write_attribution_errors, 0);

        monitor.record_completion(1, 64, "funct7_64", 20, 0x8000_4000);
        monitor.finalize_completed_instructions();
        assert_eq!(monitor.bank_target_checks, 1);
        assert_eq!(monitor.bank_target_mismatches, 0);
    }

    #[test]
    fn bank_target_check_reports_missing_and_unexpected_banks() {
        let mut monitor = BankStabilityMonitor::new();
        let wr_vbank = 4;
        monitor.apply_mset(wr_vbank, (2 << 5) | (1 << 10));
        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 1,
                domain_id: 3,
                funct: 64,
                pc: 0x8000_5000,
                rs1: wr_vbank << 20,
                rs2: 0,
                bank_enable: 4,
            },
            10,
        );
        monitor.record_decoded_writer(1, wr_vbank);

        monitor.record_actual_write(wr_vbank as u32, 0, 1);
        monitor.record_actual_write(wr_vbank as u32, 2, 2);
        monitor.record_completion(1, 64, "funct7_64", 20, 0x8000_5000);
        monitor.finalize_completed_instructions();

        assert_eq!(monitor.bank_target_checks, 1);
        assert_eq!(monitor.bank_target_mismatches, 1);
    }

    #[test]
    fn mset_initialization_is_not_a_bank_target_check() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 1,
                domain_id: 1,
                funct: 32,
                pc: 0x8000_6000,
                rs1: 4,
                rs2: (2 << 5) | (1 << 10),
                bank_enable: 2,
            },
            10,
        );
        monitor.record_decoded_writer(1, 4);
        monitor.record_completion(1, 32, "funct7_32", 20, 0x8000_6000);
        monitor.finalize_completed_instructions();

        assert_eq!(monitor.bank_target_checks, 0);
        assert_eq!(monitor.bank_target_mismatches, 0);
    }

    #[test]
    fn boot_writes_are_not_bank_target_checks() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.apply_mset(0, (1 << 5) | (1 << 10));
        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 1,
                domain_id: 1,
                funct: 33,
                pc: 0,
                rs1: 0,
                rs2: 0,
                bank_enable: 2,
            },
            10,
        );
        monitor.record_decoded_writer(1, 0);
        monitor.record_write_request(0);
        monitor.record_completion(1, 33, "funct7_33", 20, 0);
        monitor.finalize_completed_instructions();

        assert_eq!(monitor.bank_target_checks, 0);
        assert_eq!(monitor.bank_target_mismatches, 0);
    }

    #[test]
    fn implementation_local_write_without_a_writer_is_not_an_attribution_error() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.next_op_id = 1;

        monitor.record_actual_write(7, 3, 9);

        assert_eq!(monitor.bank_write_attribution_errors, 0);
        assert_eq!(monitor.write_request_counts[3], 1);
    }

    #[test]
    fn write_with_multiple_matching_vbank_writers_is_an_attribution_error() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.next_op_id = 2;
        for rob_id in [1, 2] {
            monitor.producer_metadata.insert(
                rob_id,
                ProducerMeta {
                    rob_id,
                    instruction_id: rob_id,
                    domain_id: 3,
                    comparable_seq: None,
                    funct7: 64,
                    op_type: "funct7_64".to_string(),
                    pc: 0x8000_0000 + rob_id,
                    bank_enable: 4,
                    target_vbank_id: Some(7),
                    affected_bank_set: BTreeSet::from([3]),
                    actual_bank_set: BTreeSet::new(),
                    affected_bank_source: AffectedBankSource::RtlPrivateMapping,
                    alloc_cycle: rob_id,
                    complete_cycle: None,
                    writer_source: WriterSource::VectorUnit,
                    bank_target_check_policy: BankTargetCheckPolicy::Required,
                },
            );
        }

        monitor.record_actual_write(7, 3, 9);

        assert_eq!(monitor.bank_write_attribution_errors, 1);
    }

    #[test]
    fn unavailable_decoded_bank_access_is_an_attribution_error() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.record_allocation(
            &ITraceEvent {
                is_issue: 2,
                rob_id: 1,
                domain_id: 3,
                funct: 64,
                pc: 0x8000_7000,
                rs1: 0,
                rs2: 0,
                bank_enable: 4,
            },
            10,
        );
        monitor.record_issue(&ITraceEvent {
            is_issue: 1,
            rob_id: 1,
            domain_id: 3,
            funct: 64,
            pc: 0x8000_7000,
            rs1: 0,
            rs2: 0,
            bank_enable: 4,
        });

        assert_eq!(monitor.bank_write_attribution_errors, 1);
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;

    fn allocation(rob_id: u32) -> ITraceEvent {
        ITraceEvent {
            is_issue: 2,
            rob_id,
            domain_id: 3,
            funct: 64,
            pc: 0x8000_0000 + u64::from(rob_id),
            rs1: 0,
            rs2: 0,
            bank_enable: 0,
        }
    }

    #[test]
    fn write_end_does_not_stabilize_with_outstanding_writes() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.record_allocation(&allocation(1), 1);
        monitor.record_write_dispatch(1);
        monitor.record_write_end(1);

        assert!(monitor.finalize_completed_instructions().is_empty());
        assert_eq!(monitor.producer_metadata[&1].outstanding_writes, 1);
    }

    #[test]
    fn cancelled_context_is_retained_until_dispatched_writes_drain() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.apply_mset(0, (1 << 5) | (1 << 10));
        monitor.record_allocation(&allocation(1), 1);
        monitor.record_decoded_writer(1, 0);
        monitor.record_write_dispatch(1);
        monitor.record_cancel(1);
        assert!(monitor.finalize_completed_instructions().is_empty());

        monitor.record_explicit_visible_write(1, 0, 0, 0);
        let tasks = monitor.finalize_completed_instructions();
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].boundary.cancelled);
        assert!(!monitor.producer_metadata.contains_key(&1));
    }

    #[test]
    fn late_write_and_producer_reuse_are_protocol_errors() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.apply_mset(0, (1 << 5) | (1 << 10));
        monitor.record_allocation(&allocation(1), 1);
        monitor.record_decoded_writer(1, 0);
        monitor.record_write_end(1);
        monitor.record_explicit_visible_write(1, 0, 0, 0);
        monitor.record_allocation(&allocation(1), 2);
        assert_eq!(monitor.bank_write_attribution_errors, 2);
    }

    #[test]
    fn completion_emits_boundary_for_no_write_instruction() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.record_allocation(&allocation(1), 1);
        monitor.record_completion(1, 64, "funct7_64", 2, allocation(1).pc);
        let tasks = monitor.finalize_completed_instructions();
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].boundary.expected_banks.is_empty());
        assert!(tasks[0].boundary.actual_banks.is_empty());
        assert!(tasks[0].boundary.writes.is_empty());
        assert_eq!(tasks[0].boundary.semantic_seq, 1);
    }

    #[test]
    fn bank_target_check_defers_to_boundary_comparator_after_mapping_release() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.record_allocation(&allocation(1), 1);
        let producer = monitor.producer_metadata.get_mut(&1).unwrap();
        producer.target_vbank_id = Some(4);
        producer.actual_logical_banks.insert(128);

        monitor.record_completion(1, 64, "funct7_64", 2, allocation(1).pc);
        monitor.finalize_completed_instructions();

        assert_eq!(monitor.bank_target_checks, 1);
        assert_eq!(monitor.bank_target_mismatches, 0);
    }

    #[test]
    fn logical_bank_id_is_vbank_and_group_stable() {
        assert_eq!(logical_bank_id(0, 3), 3);
        assert_eq!(logical_bank_id(4, 3), 4 * BANK_NUM as u32 + 3);
    }

    #[test]
    fn overlapping_write_ownership_is_rejected_at_issue() {
        let mut monitor = BankStabilityMonitor::new();
        monitor.apply_mset(0, (1 << 5) | (1 << 10));
        monitor.record_allocation(&allocation(1), 1);
        monitor.record_decoded_writer(1, 0);
        monitor.record_allocation(&allocation(2), 2);
        monitor.record_decoded_writer(2, 0);

        assert_eq!(monitor.bank_write_attribution_errors, 1);
        assert_eq!(monitor.bank_writers[0].len(), 1);
    }
}
