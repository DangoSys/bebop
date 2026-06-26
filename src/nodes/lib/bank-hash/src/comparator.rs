use crate::{BankHashEventClass, BankHashSource, CanonicalBankHashPacket};
use serde::Serialize;
#[cfg(test)]
use serde_json::Value;
use snafu::{ResultExt, Whatever};
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
}

impl BankHashCompareSummary {
    pub fn total(&self) -> u64 {
        self.pass + self.mismatch + self.missing_rtl + self.missing_bemu
    }

    fn add_packet(&mut self, packet: &ComparePacket) {
        match packet.result {
            CompareResult::Pass => self.pass += 1,
            CompareResult::Mismatch => self.mismatch += 1,
            CompareResult::MissingRtl => self.missing_rtl += 1,
            CompareResult::MissingBemu => self.missing_bemu += 1,
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CompareKey {
    comparable_seq: u64,
    bank_id: u32,
    version: u32,
}

#[derive(Clone, Debug)]
struct CompareRecord {
    original_instruction_id: Option<u64>,
    funct7: Option<u32>,
    op_type: Option<String>,
    hash: u64,
    cycle: Option<u64>,
    verilator_time: Option<u64>,
    pc: Option<u64>,
    original_record_ref: Option<String>,
    line_no: u64,
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
    comparable_seq: u64,
    bank_id: u32,
    version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtl_original_instruction_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bemu_original_instruction_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    funct7: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    op_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtl_hash: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bemu_hash: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtl_cycle: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bemu_cycle: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtl_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bemu_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtl_pc: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bemu_pc: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtl_record_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bemu_record_ref: Option<String>,
}

pub fn run_online_with_summary(
    packets: Receiver<CanonicalBankHashPacket>,
    output: PathBuf,
) -> Result<BankHashCompareSummary, Whatever> {
    let mut comparator = StreamingComparator::new(create_compare_writer(&output)?, output.clone());

    for packet in packets {
        comparator.ingest_packet(packet)?;
    }

    let summary = comparator.finish()?;
    println!("Online bank hash compare: {}", output.display());
    Ok(summary)
}

fn compare_records(
    rtl: &BTreeMap<CompareKey, CompareRecord>,
    bemu: &BTreeMap<CompareKey, CompareRecord>,
) -> Vec<ComparePacket> {
    let keys: BTreeSet<_> = rtl.keys().chain(bemu.keys()).cloned().collect();

    keys.into_iter()
        .map(|key| compare_record_pair(&key, rtl.get(&key), bemu.get(&key)))
        .collect()
}

fn compare_record_pair(
    key: &CompareKey,
    rtl_record: Option<&CompareRecord>,
    bemu_record: Option<&CompareRecord>,
) -> ComparePacket {
    let result = match (rtl_record, bemu_record) {
        (Some(rtl), Some(bemu)) if rtl.hash == bemu.hash => CompareResult::Pass,
        (Some(_), Some(_)) => CompareResult::Mismatch,
        (None, Some(_)) => CompareResult::MissingRtl,
        (Some(_), None) => CompareResult::MissingBemu,
        (None, None) => unreachable!("key set is derived from existing records"),
    };

    ComparePacket {
        record_type: "bank_hash_compare",
        result,
        comparable_seq: key.comparable_seq,
        bank_id: key.bank_id,
        version: key.version,
        rtl_original_instruction_id: rtl_record.and_then(|r| r.original_instruction_id),
        bemu_original_instruction_id: bemu_record.and_then(|r| r.original_instruction_id),
        funct7: bemu_record
            .and_then(|r| r.funct7)
            .or_else(|| rtl_record.and_then(|r| r.funct7)),
        op_type: bemu_record
            .and_then(|r| r.op_type.clone())
            .or_else(|| rtl_record.and_then(|r| r.op_type.clone())),
        rtl_hash: rtl_record.map(|r| r.hash),
        bemu_hash: bemu_record.map(|r| r.hash),
        rtl_cycle: rtl_record.and_then(|r| r.cycle),
        bemu_cycle: bemu_record.and_then(|r| r.cycle),
        rtl_time: rtl_record.and_then(|r| r.verilator_time),
        bemu_time: bemu_record.and_then(|r| r.verilator_time),
        rtl_pc: rtl_record.and_then(|r| r.pc),
        bemu_pc: bemu_record.and_then(|r| r.pc),
        rtl_record_ref: rtl_record.map(|r| {
            r.original_record_ref
                .clone()
                .unwrap_or_else(|| format!("line {}", r.line_no))
        }),
        bemu_record_ref: bemu_record.map(|r| {
            r.original_record_ref
                .clone()
                .unwrap_or_else(|| format!("line {}", r.line_no))
        }),
    }
}

fn create_compare_writer(path: &Path) -> Result<BufWriter<File>, Whatever> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .whatever_context(format!("failed to create output directory {}", parent.display()))?;
    }

    let file = File::create(path).whatever_context(format!("failed to create {}", path.display()))?;
    Ok(BufWriter::new(file))
}

fn write_compare_packet(writer: &mut BufWriter<File>, packet: &ComparePacket, path: &Path) -> Result<(), Whatever> {
    serde_json::to_writer(&mut *writer, packet).whatever_context(format!("failed to write {}", path.display()))?;
    writer
        .write_all(b"\n")
        .whatever_context(format!("failed to write {}", path.display()))?;
    writer
        .flush()
        .whatever_context(format!("failed to flush {}", path.display()))?;
    Ok(())
}

struct StreamingComparator {
    rtl: BTreeMap<CompareKey, CompareRecord>,
    bemu: BTreeMap<CompareKey, CompareRecord>,
    emitted: BTreeSet<CompareKey>,
    writer: BufWriter<File>,
    output_path: PathBuf,
    summary: BankHashCompareSummary,
}

impl StreamingComparator {
    fn new(writer: BufWriter<File>, output_path: PathBuf) -> Self {
        Self {
            rtl: BTreeMap::new(),
            bemu: BTreeMap::new(),
            emitted: BTreeSet::new(),
            writer,
            output_path,
            summary: BankHashCompareSummary::default(),
        }
    }

    fn ingest_packet(&mut self, packet: CanonicalBankHashPacket) -> Result<(), Whatever> {
        if packet.event_class != BankHashEventClass::BankDataWrite {
            return Ok(());
        }

        let Some(comparable_seq) = packet.comparable_seq else {
            eprintln!("warning: skipping online bank hash packet: bank_data_write missing comparable_seq");
            return Ok(());
        };

        let key = CompareKey {
            comparable_seq,
            bank_id: packet.bank_id,
            version: packet.version,
        };
        if self.emitted.contains(&key) {
            eprintln!("warning: duplicate online btrace key after compare; ignoring");
            return Ok(());
        }

        let record = CompareRecord {
            original_instruction_id: Some(packet.original_instruction_id),
            funct7: Some(packet.funct7),
            op_type: Some(packet.op_type),
            hash: packet.hash,
            cycle: packet.cycle,
            verilator_time: packet.verilator_time,
            pc: packet.pc,
            original_record_ref: Some(packet.original_record_ref),
            line_no: packet.original_log_line,
        };

        match packet.source {
            BankHashSource::Rtl => {
                self.rtl.insert(key.clone(), record);
            }
            BankHashSource::Bemu => {
                self.bemu.insert(key.clone(), record);
            }
        }

        if self.rtl.contains_key(&key) && self.bemu.contains_key(&key) {
            let rtl = self.rtl.remove(&key).expect("checked above");
            let bemu = self.bemu.remove(&key).expect("checked above");
            let packet = compare_record_pair(&key, Some(&rtl), Some(&bemu));
            write_compare_packet(&mut self.writer, &packet, &self.output_path)?;
            self.summary.add_packet(&packet);
            self.emitted.insert(key);
        }

        Ok(())
    }

    fn finish(mut self) -> Result<BankHashCompareSummary, Whatever> {
        let missing = compare_records(&self.rtl, &self.bemu);
        for packet in &missing {
            write_compare_packet(&mut self.writer, packet, &self.output_path)?;
            self.summary.add_packet(packet);
        }
        self.writer
            .flush()
            .whatever_context("failed to flush online bank hash compare output")?;
        Ok(self.summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seq: u64, bank_id: u32, version: u32) -> CompareKey {
        CompareKey {
            comparable_seq: seq,
            bank_id,
            version,
        }
    }

    fn record(seq: u64, _bank_id: u32, _version: u32, hash: u64) -> CompareRecord {
        CompareRecord {
            original_instruction_id: Some(seq),
            funct7: Some(33),
            op_type: Some("funct7_33".to_string()),
            hash,
            cycle: Some(seq * 10),
            verilator_time: None,
            pc: Some(0x8000_0000 + seq),
            original_record_ref: Some(format!("line {seq}")),
            line_no: seq,
        }
    }

    #[test]
    fn compare_reports_pass_mismatch_and_missing() {
        let mut rtl = BTreeMap::new();
        rtl.insert(key(1, 0, 0), record(1, 0, 0, 10));
        rtl.insert(key(2, 0, 0), record(2, 0, 0, 20));
        rtl.insert(key(3, 0, 0), record(3, 0, 0, 30));

        let mut bemu = BTreeMap::new();
        bemu.insert(key(1, 0, 0), record(1, 0, 0, 10));
        bemu.insert(key(2, 0, 0), record(2, 0, 0, 21));
        bemu.insert(key(4, 0, 0), record(4, 0, 0, 40));

        let packets = compare_records(&rtl, &bemu);
        let results: Vec<_> = packets.iter().map(|p| p.result.clone()).collect();

        assert_eq!(
            results,
            vec![
                CompareResult::Pass,
                CompareResult::Mismatch,
                CompareResult::MissingBemu,
                CompareResult::MissingRtl
            ]
        );
    }

    #[test]
    fn compare_ignores_original_instruction_ids() {
        let mut rtl = BTreeMap::new();
        let mut rtl_record = record(4, 0, 0, 3746813360834562347);
        rtl_record.original_instruction_id = Some(4);
        rtl.insert(key(1, 0, 0), rtl_record);

        let mut bemu = BTreeMap::new();
        let mut bemu_record = record(2, 0, 0, 3746813360834562347);
        bemu_record.original_instruction_id = Some(2);
        bemu.insert(key(1, 0, 0), bemu_record);

        let packets = compare_records(&rtl, &bemu);

        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].result, CompareResult::Pass);
        assert_eq!(packets[0].comparable_seq, 1);
        assert_eq!(packets[0].bank_id, 0);
        assert_eq!(packets[0].version, 0);
        assert_eq!(packets[0].rtl_original_instruction_id, Some(4));
        assert_eq!(packets[0].bemu_original_instruction_id, Some(2));
    }

    #[test]
    fn online_compare_reports_pass_and_missing() {
        let output = std::env::temp_dir().join(format!("bebop-bank-hash-online-{}-{}.ndjson", std::process::id(), 1));
        let writer = create_compare_writer(&output).unwrap();
        let mut comparator = StreamingComparator::new(writer, output.clone());

        comparator
            .ingest_packet(CanonicalBankHashPacket::new(
                BankHashSource::Rtl,
                4,
                Some(1),
                0,
                33,
                "funct7_33",
                BankHashEventClass::BankDataWrite,
                3746813360834562347,
                crate::BankHashTime::Cycle(10),
                Some(2147486388),
                "rtl_bank_hash.ndjson:33",
                33,
            ))
            .unwrap();
        comparator
            .ingest_packet(CanonicalBankHashPacket::new(
                BankHashSource::Bemu,
                2,
                Some(1),
                0,
                33,
                "funct7_33",
                BankHashEventClass::BankDataWrite,
                3746813360834562347,
                crate::BankHashTime::Cycle(2),
                Some(2147486388),
                "bemu_bank_hash.ndjson:2",
                2,
            ))
            .unwrap();
        comparator
            .ingest_packet(CanonicalBankHashPacket::new(
                BankHashSource::Rtl,
                5,
                Some(2),
                0,
                33,
                "funct7_33",
                BankHashEventClass::BankDataWrite,
                123,
                crate::BankHashTime::Cycle(20),
                Some(2147486400),
                "rtl_bank_hash.ndjson:34",
                34,
            ))
            .unwrap();

        let summary = comparator.finish().unwrap();
        let lines = std::fs::read_to_string(&output).unwrap();
        let values: Vec<Value> = lines.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        std::fs::remove_file(&output).ok();

        assert_eq!(
            summary,
            BankHashCompareSummary {
                pass: 1,
                mismatch: 0,
                missing_rtl: 0,
                missing_bemu: 1
            }
        );
        assert_eq!(values.len(), 2);
        assert_eq!(values[0]["result"], "PASS");
        assert_eq!(values[0]["comparable_seq"], 1);
        assert_eq!(values[1]["result"], "MISSING_BEMU");
        assert_eq!(values[1]["comparable_seq"], 2);
    }
}
