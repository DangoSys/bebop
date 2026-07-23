use crate::{
    BankDigest, BankHashEventClass, BankHashSource, CanonicalBankHashPacket, InstructionBankBoundaryPacket,
    RuntimeBankDifftestEvent,
};
use serde::Serialize;
use snafu::{FromString, ResultExt, Whatever};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BankHashCompareSummary {
    pub pass: u64,
    pub mismatch: u64,
    pub missing_rtl: u64,
    pub missing_bemu: u64,
    pub target_mismatch: u64,
    pub dependency_mismatch: u64,
    pub protocol_errors: u64,
}

impl BankHashCompareSummary {
    pub fn total(&self) -> u64 {
        self.pass
            + self.mismatch
            + self.missing_rtl
            + self.missing_bemu
            + self.target_mismatch
            + self.dependency_mismatch
            + self.protocol_errors
    }

    pub fn is_clean(&self) -> bool {
        self.mismatch == 0
            && self.missing_rtl == 0
            && self.missing_bemu == 0
            && self.target_mismatch == 0
            && self.dependency_mismatch == 0
            && self.protocol_errors == 0
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CompareKey {
    op_id: u64,
    bank_id: u32,
    bank_version: u32,
}

#[derive(Clone, Debug)]
struct CompareRecord {
    instruction_id: u64,
    funct7: u32,
    op_type: String,
    hash: u64,
    cycle: Option<u64>,
    verilator_time: Option<u64>,
    pc: Option<u64>,
    record_ref: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum CompareResult {
    Pass,
    Mismatch,
    MissingRtl,
    MissingBemu,
}

#[derive(Clone, Debug, Serialize)]
struct ComparePacket {
    #[serde(rename = "type")]
    record_type: &'static str,
    result: CompareResult,
    op_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtl_instruction_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    golden_instruction_id: Option<u64>,
    bank_id: u32,
    bank_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    funct7: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    op_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtl_hash: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    golden_hash: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtl_cycle: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    golden_cycle: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtl_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtl_pc: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    golden_pc: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtl_record_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    golden_record_ref: Option<String>,
}

#[derive(Serialize)]
struct BoundaryComparePacket {
    #[serde(rename = "type")]
    record_type: &'static str,
    result: &'static str,
    semantic_seq: u64,
    rtl_instruction_id: u64,
    golden_instruction_id: u64,
    expected_banks: Vec<u32>,
    rtl_declared_banks: Vec<u32>,
    actual_banks: Vec<u32>,
    missing_banks: Vec<u32>,
    unexpected_banks: Vec<u32>,
    rtl_reads: Vec<crate::BankVersionRef>,
    golden_reads: Vec<crate::BankVersionRef>,
    dependency_match: bool,
}

pub fn run_online_with_summary(
    packets: Receiver<RuntimeBankDifftestEvent>,
    output: PathBuf,
) -> Result<BankHashCompareSummary, Whatever> {
    let mut comparator = StreamingComparator::new(create_compare_writer(&output)?, output.clone());
    for packet in packets {
        comparator.ingest_event(packet)?;
    }
    let summary = comparator.finish()?;
    println!("Online bank hash compare: {}", output.display());
    Ok(summary)
}

pub struct SynchronousBankComparator {
    inner: StreamingComparator,
}

impl SynchronousBankComparator {
    pub fn new(output: PathBuf) -> Result<Self, Whatever> {
        Ok(Self {
            inner: StreamingComparator::new(create_compare_writer(&output)?, output),
        })
    }

    pub fn poll(&mut self, packets: &Receiver<RuntimeBankDifftestEvent>) -> Result<(), Whatever> {
        for packet in packets.try_iter() {
            self.inner.ingest_event(packet)?;
        }
        Ok(())
    }

    pub fn finish(self) -> Result<BankHashCompareSummary, Whatever> {
        self.inner.finish()
    }
}

fn create_compare_writer(path: &Path) -> Result<BufWriter<File>, Whatever> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .whatever_context(format!("failed to create output directory {}", parent.display()))?;
    }
    File::create(path)
        .map(BufWriter::new)
        .whatever_context(format!("failed to create {}", path.display()))
}

fn write_packet<T: Serialize>(writer: &mut BufWriter<File>, packet: &T, path: &Path) -> Result<(), Whatever> {
    serde_json::to_writer(&mut *writer, packet).whatever_context(format!("failed to write {}", path.display()))?;
    writer
        .write_all(b"\n")
        .whatever_context(format!("failed to write {}", path.display()))
}

fn protocol_error(message: String) -> Whatever {
    crate::report_runtime_bank_difftest_failure();
    Whatever::without_source(message)
}

struct StreamingComparator {
    rtl_hashes: BTreeMap<CompareKey, CompareRecord>,
    bemu_hashes: BTreeMap<CompareKey, CompareRecord>,
    emitted_hashes: BTreeSet<CompareKey>,
    rtl_boundaries: BTreeMap<u64, InstructionBankBoundaryPacket>,
    bemu_boundaries: BTreeMap<u64, InstructionBankBoundaryPacket>,
    emitted_boundaries: BTreeSet<u64>,
    writer: BufWriter<File>,
    output_path: PathBuf,
    summary: BankHashCompareSummary,
    saw_fatal_failure: bool,
}

impl StreamingComparator {
    fn new(writer: BufWriter<File>, output_path: PathBuf) -> Self {
        Self {
            rtl_hashes: BTreeMap::new(),
            bemu_hashes: BTreeMap::new(),
            emitted_hashes: BTreeSet::new(),
            rtl_boundaries: BTreeMap::new(),
            bemu_boundaries: BTreeMap::new(),
            emitted_boundaries: BTreeSet::new(),
            writer,
            output_path,
            summary: BankHashCompareSummary::default(),
            saw_fatal_failure: false,
        }
    }

    fn ingest_event(&mut self, event: RuntimeBankDifftestEvent) -> Result<(), Whatever> {
        match event {
            RuntimeBankDifftestEvent::Hash(packet) => self.ingest_hash(packet),
            RuntimeBankDifftestEvent::Boundary(packet) => self.ingest_boundary(packet),
        }
    }

    fn ingest_hash(&mut self, packet: CanonicalBankHashPacket) -> Result<(), Whatever> {
        if packet.event_class != BankHashEventClass::BankDataWrite {
            return Ok(());
        }
        let op_id = packet.comparable_seq.ok_or_else(|| {
            protocol_error(format!(
                "BankDataWrite missing semantic sequence: source={:?} instruction_id={}",
                packet.source, packet.original_instruction_id
            ))
        })?;
        let key = CompareKey {
            op_id,
            bank_id: packet.bank_id,
            bank_version: packet.version,
        };
        if self.emitted_hashes.contains(&key) {
            return Err(protocol_error(format!(
                "duplicate bank hash after pairing: op_id={} bank_id={} version={}",
                key.op_id, key.bank_id, key.bank_version
            )));
        }
        let source = packet.source;
        let record = CompareRecord {
            instruction_id: packet.original_instruction_id,
            funct7: packet.funct7,
            op_type: packet.op_type,
            hash: packet.hash,
            cycle: packet.cycle,
            verilator_time: packet.verilator_time,
            pc: packet.pc,
            record_ref: packet.original_record_ref,
        };
        let side = match source {
            BankHashSource::Rtl => &mut self.rtl_hashes,
            BankHashSource::Bemu => &mut self.bemu_hashes,
        };
        if side.insert(key.clone(), record).is_some() {
            return Err(protocol_error(format!(
                "duplicate bank hash: source={source:?} op_id={} bank_id={} version={}",
                key.op_id, key.bank_id, key.bank_version
            )));
        }
        Ok(())
    }

    fn ingest_boundary(&mut self, mut packet: InstructionBankBoundaryPacket) -> Result<(), Whatever> {
        packet.normalize();
        if packet.semantic_seq == 0 {
            return Err(protocol_error(
                "instruction Bank boundary has semantic_seq=0".to_string(),
            ));
        }
        let seq = packet.semantic_seq;
        if self.emitted_boundaries.contains(&seq) {
            return Err(protocol_error(format!(
                "duplicate instruction Bank boundary after pairing: semantic_seq={seq}"
            )));
        }
        let source = packet.source;
        let side = match source {
            BankHashSource::Rtl => &mut self.rtl_boundaries,
            BankHashSource::Bemu => &mut self.bemu_boundaries,
        };
        if side.insert(seq, packet).is_some() {
            return Err(protocol_error(format!(
                "duplicate instruction Bank boundary: source={source:?} semantic_seq={seq}"
            )));
        }
        if self.rtl_boundaries.contains_key(&seq) && self.bemu_boundaries.contains_key(&seq) {
            let rtl = self.rtl_boundaries.remove(&seq).expect("checked");
            let bemu = self.bemu_boundaries.remove(&seq).expect("checked");
            self.compare_boundary(rtl, bemu)?;
            self.emitted_boundaries.insert(seq);
        }
        Ok(())
    }

    fn compare_boundary(
        &mut self,
        rtl: InstructionBankBoundaryPacket,
        bemu: InstructionBankBoundaryPacket,
    ) -> Result<(), Whatever> {
        let expected: BTreeSet<_> = bemu.expected_banks.iter().copied().collect();
        let actual: BTreeSet<_> = rtl.actual_banks.iter().copied().collect();
        let rtl_declared: BTreeSet<_> = rtl.expected_banks.iter().copied().collect();
        let missing: Vec<_> = expected.difference(&actual).copied().collect();
        let unexpected: Vec<_> = actual.difference(&expected).copied().collect();
        let dependency_match = rtl.reads == bemu.reads;
        // The architectural structural check is T_act == T_exp. The RTL
        // decoded/hazard declaration is retained for diagnostics, but it is
        // not an additional architectural target set.
        let target_match = missing.is_empty() && unexpected.is_empty();

        let boundary_packet = BoundaryComparePacket {
            record_type: "bank_boundary_compare",
            result: if target_match && dependency_match {
                "PASS"
            } else {
                "MISMATCH"
            },
            semantic_seq: rtl.semantic_seq,
            rtl_instruction_id: rtl.instruction_id,
            golden_instruction_id: bemu.instruction_id,
            expected_banks: expected.iter().copied().collect(),
            rtl_declared_banks: rtl_declared.iter().copied().collect(),
            actual_banks: actual.iter().copied().collect(),
            missing_banks: missing,
            unexpected_banks: unexpected,
            rtl_reads: rtl.reads.clone(),
            golden_reads: bemu.reads.clone(),
            dependency_match,
        };
        write_packet(&mut self.writer, &boundary_packet, &self.output_path)?;

        if !target_match {
            self.summary.target_mismatch += 1;
            self.saw_fatal_failure = true;
            crate::report_runtime_bank_difftest_failure();
            return Ok(());
        }
        if !dependency_match {
            self.summary.dependency_mismatch += 1;
            self.saw_fatal_failure = true;
            crate::report_runtime_bank_difftest_failure();
            return Ok(());
        }

        let rtl_writes: BTreeMap<_, _> = rtl
            .writes
            .iter()
            .map(|entry| ((entry.bank_id, entry.version), *entry))
            .collect();
        let bemu_writes: BTreeMap<_, _> = bemu
            .writes
            .iter()
            .map(|entry| ((entry.bank_id, entry.version), *entry))
            .collect();
        let keys: BTreeSet<_> = rtl_writes.keys().chain(bemu_writes.keys()).copied().collect();
        for (bank_id, version) in keys {
            let rtl_digest = rtl_writes.get(&(bank_id, version));
            let bemu_digest = bemu_writes.get(&(bank_id, version));
            self.emit_digest_compare(&rtl, &bemu, bank_id, version, rtl_digest, bemu_digest)?;
        }
        Ok(())
    }

    fn emit_digest_compare(
        &mut self,
        rtl: &InstructionBankBoundaryPacket,
        bemu: &InstructionBankBoundaryPacket,
        bank_id: u32,
        version: u32,
        rtl_digest: Option<&BankDigest>,
        bemu_digest: Option<&BankDigest>,
    ) -> Result<(), Whatever> {
        let result = match (rtl_digest, bemu_digest) {
            (Some(rtl), Some(bemu)) if rtl.hash == bemu.hash => CompareResult::Pass,
            (Some(_), Some(_)) => CompareResult::Mismatch,
            (None, Some(_)) => CompareResult::MissingRtl,
            (Some(_), None) => CompareResult::MissingBemu,
            (None, None) => unreachable!(),
        };
        let packet = ComparePacket {
            record_type: "bank_hash_compare",
            result,
            op_id: rtl.semantic_seq,
            rtl_instruction_id: Some(rtl.instruction_id),
            golden_instruction_id: Some(bemu.instruction_id),
            bank_id,
            bank_version: version,
            funct7: Some(bemu.funct7),
            op_type: Some(format!("funct7_{}", bemu.funct7)),
            rtl_hash: rtl_digest.map(|entry| entry.hash),
            golden_hash: bemu_digest.map(|entry| entry.hash),
            rtl_cycle: Some(rtl.cycle),
            golden_cycle: Some(bemu.cycle),
            rtl_time: None,
            rtl_pc: Some(rtl.pc),
            golden_pc: Some(bemu.pc),
            rtl_record_ref: None,
            golden_record_ref: None,
        };
        write_packet(&mut self.writer, &packet, &self.output_path)?;
        match result {
            CompareResult::Pass => self.summary.pass += 1,
            CompareResult::Mismatch => self.summary.mismatch += 1,
            CompareResult::MissingRtl => self.summary.missing_rtl += 1,
            CompareResult::MissingBemu => self.summary.missing_bemu += 1,
        }
        if result != CompareResult::Pass {
            self.saw_fatal_failure = true;
            crate::report_runtime_bank_difftest_failure();
        }
        Ok(())
    }

    fn finish(mut self) -> Result<BankHashCompareSummary, Whatever> {
        if !self.saw_fatal_failure && !crate::runtime_bank_difftest_failure_detected() {
            let missing_boundaries = self.rtl_boundaries.len() + self.bemu_boundaries.len();
            if missing_boundaries != 0 {
                self.summary.protocol_errors += missing_boundaries as u64;
                crate::report_runtime_bank_difftest_failure();
            }

            let hash_keys: BTreeSet<_> = self.rtl_hashes.keys().chain(self.bemu_hashes.keys()).cloned().collect();
            for key in hash_keys {
                let rtl = self.rtl_hashes.get(&key);
                let bemu = self.bemu_hashes.get(&key);
                let result = match (rtl, bemu) {
                    (Some(rtl), Some(bemu)) if rtl.hash == bemu.hash => CompareResult::Pass,
                    (Some(_), Some(_)) => CompareResult::Mismatch,
                    (None, Some(_)) => CompareResult::MissingRtl,
                    (Some(_), None) => CompareResult::MissingBemu,
                    (None, None) => unreachable!(),
                };
                let packet = ComparePacket {
                    record_type: "bank_hash_compare",
                    result,
                    op_id: key.op_id,
                    rtl_instruction_id: rtl.map(|entry| entry.instruction_id),
                    golden_instruction_id: bemu.map(|entry| entry.instruction_id),
                    bank_id: key.bank_id,
                    bank_version: key.bank_version,
                    funct7: bemu.map(|entry| entry.funct7).or_else(|| rtl.map(|entry| entry.funct7)),
                    op_type: bemu
                        .map(|entry| entry.op_type.clone())
                        .or_else(|| rtl.map(|entry| entry.op_type.clone())),
                    rtl_hash: rtl.map(|entry| entry.hash),
                    golden_hash: bemu.map(|entry| entry.hash),
                    rtl_cycle: rtl.and_then(|entry| entry.cycle),
                    golden_cycle: bemu.and_then(|entry| entry.cycle),
                    rtl_time: rtl.and_then(|entry| entry.verilator_time),
                    rtl_pc: rtl.and_then(|entry| entry.pc),
                    golden_pc: bemu.and_then(|entry| entry.pc),
                    rtl_record_ref: rtl.map(|entry| entry.record_ref.clone()),
                    golden_record_ref: bemu.map(|entry| entry.record_ref.clone()),
                };
                write_packet(&mut self.writer, &packet, &self.output_path)?;
                match result {
                    CompareResult::Pass => self.summary.pass += 1,
                    CompareResult::Mismatch => self.summary.mismatch += 1,
                    CompareResult::MissingRtl => self.summary.missing_rtl += 1,
                    CompareResult::MissingBemu => self.summary.missing_bemu += 1,
                }
            }
        }
        self.writer
            .flush()
            .whatever_context("failed to flush online bank compare output")?;
        Ok(self.summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BankHashTime, BankVersionRef};

    fn boundary(source: BankHashSource, hash: u64) -> InstructionBankBoundaryPacket {
        InstructionBankBoundaryPacket {
            record_type: "instruction_bank_boundary",
            source,
            instruction_id: 7,
            semantic_seq: 3,
            funct7: 33,
            pc: 0x8000_1000,
            expected_banks: vec![2],
            actual_banks: vec![2],
            reads: vec![BankVersionRef { bank_id: 1, version: 4 }],
            writes: vec![BankDigest {
                bank_id: 2,
                version: 5,
                hash,
            }],
            cycle: 10,
            cancelled: false,
        }
    }

    #[test]
    fn boundary_comparison_is_structural_then_content() {
        let output = std::env::temp_dir().join(format!("bebop-boundary-{}.ndjson", std::process::id()));
        let mut comparator = StreamingComparator::new(create_compare_writer(&output).unwrap(), output.clone());
        comparator.ingest_boundary(boundary(BankHashSource::Rtl, 9)).unwrap();
        comparator.ingest_boundary(boundary(BankHashSource::Bemu, 9)).unwrap();
        let summary = comparator.finish().unwrap();
        assert_eq!(summary.pass, 1);
        assert!(summary.is_clean());
        std::fs::remove_file(output).ok();
    }

    #[test]
    fn target_mismatch_stops_before_content_comparison() {
        let output = std::env::temp_dir().join(format!("bebop-target-{}.ndjson", std::process::id()));
        let mut comparator = StreamingComparator::new(create_compare_writer(&output).unwrap(), output.clone());
        let mut rtl = boundary(BankHashSource::Rtl, 9);
        rtl.actual_banks = vec![3];
        comparator.ingest_boundary(rtl).unwrap();
        comparator.ingest_boundary(boundary(BankHashSource::Bemu, 9)).unwrap();
        let summary = comparator.finish().unwrap();
        assert_eq!(summary.target_mismatch, 1);
        assert_eq!(summary.pass, 0);
        std::fs::remove_file(output).ok();
    }

    #[test]
    fn legacy_hash_without_semantic_sequence_is_protocol_error() {
        let output = std::env::temp_dir().join(format!("bebop-protocol-{}.ndjson", std::process::id()));
        let mut comparator = StreamingComparator::new(create_compare_writer(&output).unwrap(), output.clone());
        let packet = CanonicalBankHashPacket::new(
            BankHashSource::Rtl,
            1,
            None,
            0,
            33,
            "funct7_33",
            BankHashEventClass::BankDataWrite,
            1,
            BankHashTime::Cycle(1),
            Some(0x8000_0000),
            "event",
            1,
        );
        assert!(comparator.ingest_hash(packet).is_err());
        std::fs::remove_file(output).ok();
    }
}
