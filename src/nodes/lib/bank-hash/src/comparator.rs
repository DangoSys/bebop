use crate::{BankDigestRecord, BankHashEventClass, BankHashSource, LogicalBankId};
use serde::Serialize;
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
    pub unexpected_rtl: u64,
}

impl BankHashCompareSummary {
    pub fn total(&self) -> u64 {
        self.pass + self.mismatch + self.missing_rtl + self.unexpected_rtl
    }

    pub fn passed(&self) -> bool {
        self.mismatch == 0 && self.missing_rtl == 0 && self.unexpected_rtl == 0
    }

    fn add(&mut self, result: BankDigestCompareResult) {
        match result {
            BankDigestCompareResult::Pass => self.pass += 1,
            BankDigestCompareResult::Mismatch => self.mismatch += 1,
            BankDigestCompareResult::MissingRtl => self.missing_rtl += 1,
            BankDigestCompareResult::UnexpectedRtl => self.unexpected_rtl += 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CompareKey {
    instruction_id: u64,
    bank_id: LogicalBankId,
}

impl From<&BankDigestRecord> for CompareKey {
    fn from(record: &BankDigestRecord) -> Self {
        Self {
            instruction_id: record.instruction_id,
            bank_id: record.bank_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BankDigestCompareResult {
    Pass,
    Mismatch,
    MissingRtl,
    UnexpectedRtl,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BankDigestComparison {
    #[serde(rename = "type")]
    record_type: &'static str,
    pub result: BankDigestCompareResult,
    pub instruction_id: u64,
    pub bank_id: LogicalBankId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtl_digest: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bemu_digest: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtl_physical_bank_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bemu_physical_bank_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub funct7: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pc: Option<u64>,
}

fn compare_pair(
    key: CompareKey,
    rtl: Option<&BankDigestRecord>,
    bemu: Option<&BankDigestRecord>,
) -> BankDigestComparison {
    let result = match (rtl, bemu) {
        (Some(rtl), Some(bemu)) if rtl.digest == bemu.digest => BankDigestCompareResult::Pass,
        (Some(_), Some(_)) => BankDigestCompareResult::Mismatch,
        (None, Some(_)) => BankDigestCompareResult::MissingRtl,
        (Some(_), None) => BankDigestCompareResult::UnexpectedRtl,
        (None, None) => unreachable!("comparison key comes from an existing record"),
    };

    BankDigestComparison {
        record_type: "bank_digest_compare",
        result,
        instruction_id: key.instruction_id,
        bank_id: key.bank_id,
        rtl_digest: rtl.map(|record| record.digest),
        bemu_digest: bemu.map(|record| record.digest),
        rtl_physical_bank_id: rtl.and_then(|record| record.physical_bank_id),
        bemu_physical_bank_id: bemu.and_then(|record| record.physical_bank_id),
        funct7: bemu
            .map(|record| record.funct7)
            .or_else(|| rtl.map(|record| record.funct7)),
        op_type: bemu
            .map(|record| record.op_type.clone())
            .or_else(|| rtl.map(|record| record.op_type.clone())),
        pc: bemu
            .and_then(|record| record.pc)
            .or_else(|| rtl.and_then(|record| record.pc)),
    }
}

fn records_by_key(records: impl IntoIterator<Item = BankDigestRecord>) -> BTreeMap<CompareKey, BankDigestRecord> {
    records
        .into_iter()
        .filter(|record| record.event_class == BankHashEventClass::BankDataWrite)
        .map(|record| (CompareKey::from(&record), record))
        .collect()
}

/// M1 offline comparison entry point. Arrival order and physical placement do
/// not affect matching; only <InstID, LogicalBankID> forms the key.
pub fn compare_offline(
    rtl: impl IntoIterator<Item = BankDigestRecord>,
    bemu: impl IntoIterator<Item = BankDigestRecord>,
) -> Vec<BankDigestComparison> {
    let rtl = records_by_key(rtl);
    let bemu = records_by_key(bemu);
    let keys: BTreeSet<_> = rtl.keys().chain(bemu.keys()).copied().collect();
    keys.into_iter()
        .map(|key| compare_pair(key, rtl.get(&key), bemu.get(&key)))
        .collect()
}

pub fn run_online_with_summary(
    records: Receiver<BankDigestRecord>,
    output: PathBuf,
) -> Result<BankHashCompareSummary, Whatever> {
    let mut comparator = StreamingComparator::new(create_compare_writer(&output)?, output.clone());
    let mut received = 0u64;
    let mut data_writes = 0u64;
    for record in records {
        received += 1;
        if record.event_class == BankHashEventClass::BankDataWrite {
            data_writes += 1;
        }
        comparator.ingest(record)?;
    }
    let summary = comparator.finish()?;
    println!(
        "Online bank digest compare: {} (received={} data_writes={})",
        output.display(),
        received,
        data_writes
    );
    Ok(summary)
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

fn write_comparison(
    writer: &mut BufWriter<File>,
    comparison: &BankDigestComparison,
    path: &Path,
) -> Result<(), Whatever> {
    serde_json::to_writer(&mut *writer, comparison).whatever_context(format!("failed to write {}", path.display()))?;
    writer
        .write_all(b"\n")
        .whatever_context(format!("failed to write {}", path.display()))?;
    writer
        .flush()
        .whatever_context(format!("failed to flush {}", path.display()))?;
    Ok(())
}

struct StreamingComparator {
    rtl: BTreeMap<CompareKey, BankDigestRecord>,
    bemu: BTreeMap<CompareKey, BankDigestRecord>,
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

    fn ingest(&mut self, record: BankDigestRecord) -> Result<(), Whatever> {
        if record.event_class != BankHashEventClass::BankDataWrite {
            return Ok(());
        }
        let key = CompareKey::from(&record);
        if self.emitted.contains(&key) {
            eprintln!("warning: duplicate bank digest key after comparison; ignoring");
            return Ok(());
        }
        match record.source {
            BankHashSource::Rtl => self.rtl.insert(key, record),
            BankHashSource::Bemu => self.bemu.insert(key, record),
        };

        if self.rtl.contains_key(&key) && self.bemu.contains_key(&key) {
            let rtl = self.rtl.remove(&key).expect("checked RTL key exists");
            let bemu = self.bemu.remove(&key).expect("checked BEMU key exists");
            let comparison = compare_pair(key, Some(&rtl), Some(&bemu));
            write_comparison(&mut self.writer, &comparison, &self.output_path)?;
            self.summary.add(comparison.result);
            self.emitted.insert(key);
        }
        Ok(())
    }

    fn finish(mut self) -> Result<BankHashCompareSummary, Whatever> {
        let remaining = compare_offline(self.rtl.into_values(), self.bemu.into_values());
        for comparison in &remaining {
            write_comparison(&mut self.writer, comparison, &self.output_path)?;
            self.summary.add(comparison.result);
        }
        self.writer
            .flush()
            .whatever_context("failed to flush bank digest comparison output")?;
        Ok(self.summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bank_hash, BankHashTime};

    fn record(
        source: BankHashSource,
        inst: u64,
        vbank: u32,
        group: u32,
        physical: u32,
        bytes: &[u8],
    ) -> BankDigestRecord {
        BankDigestRecord::new(
            source,
            inst,
            LogicalBankId::new(vbank, group),
            Some(physical),
            bank_hash(bytes),
            64,
            "funct7_64",
            BankHashEventClass::BankDataWrite,
            BankHashTime::Cycle(inst * 10),
            Some(0x8000_0000 + inst),
            None,
        )
    }

    #[test]
    fn m1_offline_compare_matches_by_inst_and_logical_bank() {
        let bemu = vec![
            record(BankHashSource::Bemu, 7, 2, 0, 4, b"same"),
            record(BankHashSource::Bemu, 8, 3, 1, 5, b"golden"),
            record(BankHashSource::Bemu, 9, 4, 0, 6, b"missing"),
        ];
        let rtl = vec![
            // Deliberately out of order and in a different physical slot.
            record(BankHashSource::Rtl, 8, 3, 1, 23, b"corrupt"),
            record(BankHashSource::Rtl, 7, 2, 0, 22, b"same"),
            record(BankHashSource::Rtl, 10, 5, 0, 24, b"unexpected"),
        ];

        let comparisons = compare_offline(rtl, bemu);
        assert_eq!(comparisons.len(), 4);
        assert_eq!(comparisons[0].result, BankDigestCompareResult::Pass);
        assert_eq!(comparisons[1].result, BankDigestCompareResult::Mismatch);
        assert_eq!(comparisons[2].result, BankDigestCompareResult::MissingRtl);
        assert_eq!(comparisons[3].result, BankDigestCompareResult::UnexpectedRtl);
        assert_eq!(comparisons[0].rtl_physical_bank_id, Some(22));
        assert_eq!(comparisons[0].bemu_physical_bank_id, Some(4));
    }

    #[test]
    fn same_inst_different_groups_are_independent_records() {
        let bemu = vec![
            record(BankHashSource::Bemu, 3, 1, 0, 4, b"g0"),
            record(BankHashSource::Bemu, 3, 1, 1, 5, b"g1"),
        ];
        let rtl = vec![
            record(BankHashSource::Rtl, 3, 1, 1, 9, b"g1"),
            record(BankHashSource::Rtl, 3, 1, 0, 8, b"g0"),
        ];

        let comparisons = compare_offline(rtl, bemu);
        assert_eq!(comparisons.len(), 2);
        assert!(comparisons
            .iter()
            .all(|comparison| comparison.result == BankDigestCompareResult::Pass));
    }

    #[test]
    fn streaming_compare_keeps_the_first_side_until_its_peer_arrives() {
        let output = std::env::temp_dir().join(format!("bebop-bank-streaming-{}.ndjson", std::process::id()));
        let writer = create_compare_writer(&output).unwrap();
        let mut comparator = StreamingComparator::new(writer, output);
        comparator
            .ingest(record(BankHashSource::Rtl, 7, 2, 0, 22, b"same"))
            .unwrap();
        comparator
            .ingest(record(BankHashSource::Bemu, 7, 2, 0, 4, b"same"))
            .unwrap();
        let summary = comparator.finish().unwrap();
        assert_eq!(summary.pass, 1);
        assert_eq!(summary.total(), 1);
    }
}
