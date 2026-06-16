use serde::{Deserialize, Serialize};
use serde_json::Value;
use snafu::{FromString, ResultExt, Whatever};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct BankHashCompareCli {
    pub rtl: PathBuf,
    pub bemu: PathBuf,
    pub output: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BankHashCompareStreamCli {
    pub input: PathBuf,
    pub output: PathBuf,
    pub idle_timeout_ms: u64,
}

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

    #[allow(dead_code)]
    pub fn is_success(&self) -> bool {
        self.pass > 0 && self.mismatch == 0 && self.missing_rtl == 0 && self.missing_bemu == 0
    }

    fn add_packet(&mut self, packet: &ComparePacket) {
        match packet.result {
            CompareResult::Pass => self.pass += 1,
            CompareResult::Mismatch => self.mismatch += 1,
            CompareResult::MissingRtl => self.missing_rtl += 1,
            CompareResult::MissingBemu => self.missing_bemu += 1,
        }
    }

    fn from_packets(packets: &[ComparePacket]) -> Self {
        let mut summary = Self::default();
        for packet in packets {
            summary.add_packet(packet);
        }
        summary
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

#[derive(Debug, Deserialize)]
struct CanonicalInput {
    #[serde(default)]
    source: Option<String>,
    comparable_seq: Option<u64>,
    bank_id: u32,
    #[serde(default)]
    version: u32,
    #[serde(default)]
    event_class: Option<String>,
    #[serde(default)]
    original_instruction_id: Option<u64>,
    #[serde(default)]
    funct7: Option<u32>,
    #[serde(default)]
    op_type: Option<String>,
    #[serde(rename = "hash_u64")]
    hash: u64,
    #[serde(default)]
    cycle: Option<u64>,
    #[serde(default)]
    verilator_time: Option<u64>,
    #[serde(default)]
    pc: Option<u64>,
    #[serde(default)]
    original_record_ref: Option<String>,
}

pub fn run(cli: BankHashCompareCli) -> Result<(), Whatever> {
    let summary = run_with_summary(cli)?;
    println!(
        "Bank hash compare summary: PASS={} MISMATCH={} MISSING_RTL={} MISSING_BEMU={} TOTAL={}",
        summary.pass,
        summary.mismatch,
        summary.missing_rtl,
        summary.missing_bemu,
        summary.total()
    );
    Ok(())
}

pub fn run_with_summary(cli: BankHashCompareCli) -> Result<BankHashCompareSummary, Whatever> {
    let rtl = read_compare_records(&cli.rtl, "RTL")?;
    let bemu = read_compare_records(&cli.bemu, "BEMU")?;
    let packets = compare_records(&rtl, &bemu);
    write_compare_packets(&cli.output, &packets)?;
    println!("Bank hash compare: {}", cli.output.display());
    Ok(BankHashCompareSummary::from_packets(&packets))
}

pub fn run_stream(cli: BankHashCompareStreamCli) -> Result<(), Whatever> {
    let summary = run_stream_with_summary(cli)?;
    println!(
        "Runtime bank hash compare summary: PASS={} MISMATCH={} MISSING_RTL={} MISSING_BEMU={} TOTAL={}",
        summary.pass,
        summary.mismatch,
        summary.missing_rtl,
        summary.missing_bemu,
        summary.total()
    );
    Ok(())
}

pub fn run_stream_with_summary(cli: BankHashCompareStreamCli) -> Result<BankHashCompareSummary, Whatever> {
    let mut comparator = StreamingComparator::new(create_compare_writer(&cli.output)?, cli.output.clone());
    let idle_timeout = Duration::from_millis(cli.idle_timeout_ms);
    let file = wait_for_stream_file(&cli.input, idle_timeout)?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut line_no = 0u64;
    let mut last_progress = Instant::now();

    loop {
        line.clear();
        let bytes = reader
            .read_line(&mut line)
            .whatever_context(format!("failed to read {}", cli.input.display()))?;
        if bytes == 0 {
            if last_progress.elapsed() >= idle_timeout {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
            continue;
        }

        line_no = line_no.wrapping_add(1);
        last_progress = Instant::now();
        comparator.ingest_line(&cli.input, line_no, &line)?;
    }

    let summary = comparator.finish()?;
    println!("Runtime bank hash compare: {}", cli.output.display());
    Ok(summary)
}

fn read_compare_records(path: &Path, source_name: &str) -> Result<BTreeMap<CompareKey, CompareRecord>, Whatever> {
    let file = File::open(path).whatever_context(format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut records = BTreeMap::new();

    for (idx, line) in reader.lines().enumerate() {
        let line_no = idx as u64 + 1;
        let line = line.whatever_context(format!("failed to read {} line {}", path.display(), line_no))?;
        if line.trim().is_empty() {
            continue;
        }

        let value: Value = serde_json::from_str(&line).whatever_context(format!(
            "failed to parse {} line {}",
            path.display(),
            line_no
        ))?;
        let input: CanonicalInput = serde_json::from_value(value).whatever_context(format!(
            "failed to decode {} line {}",
            path.display(),
            line_no
        ))?;

        if input.event_class.as_deref() != Some("bank_data_write") {
            continue;
        }

        let Some(comparable_seq) = input.comparable_seq else {
            eprintln!(
                "warning: skipping {source_name} {} line {line_no}: bank_data_write missing comparable_seq",
                path.display()
            );
            continue;
        };

        let key = CompareKey {
            comparable_seq,
            bank_id: input.bank_id,
            version: input.version,
        };
        let record = CompareRecord {
            original_instruction_id: input.original_instruction_id,
            funct7: input.funct7,
            op_type: input.op_type,
            hash: input.hash,
            cycle: input.cycle,
            verilator_time: input.verilator_time,
            pc: input.pc,
            original_record_ref: input
                .original_record_ref
                .or_else(|| Some(format!("{}:{line_no}", path.display()))),
            line_no,
        };

        if records.insert(key, record).is_some() {
            eprintln!(
                "warning: duplicate {source_name} canonical key in {} line {line_no}; using later record",
                path.display()
            );
        }
    }

    Ok(records)
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

fn write_compare_packets(path: &Path, packets: &[ComparePacket]) -> Result<(), Whatever> {
    let mut writer = create_compare_writer(path)?;
    for packet in packets {
        write_compare_packet(&mut writer, packet, path)?;
    }
    writer
        .flush()
        .whatever_context(format!("failed to flush {}", path.display()))?;
    Ok(())
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

fn wait_for_stream_file(path: &Path, timeout: Duration) -> Result<File, Whatever> {
    let start = Instant::now();
    loop {
        match OpenOptions::new().read(true).open(path) {
            Ok(file) => return Ok(file),
            Err(e) if start.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(25));
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(Whatever::without_source(format!(
                        "failed to open {}: {e}",
                        path.display()
                    )));
                }
            }
            Err(e) => {
                return Err(Whatever::without_source(format!(
                    "failed to open {}: {e}",
                    path.display()
                )))
            }
        }
    }
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

    fn ingest_line(&mut self, path: &Path, line_no: u64, line: &str) -> Result<(), Whatever> {
        if line.trim().is_empty() {
            return Ok(());
        }

        let Some((source, key, record)) = parse_stream_record(path, line_no, line)? else {
            return Ok(());
        };
        if self.emitted.contains(&key) {
            eprintln!(
                "warning: duplicate runtime canonical key after compare in {} line {}; ignoring",
                path.display(),
                line_no
            );
            return Ok(());
        }

        match source.as_str() {
            "RTL" => {
                self.rtl.insert(key.clone(), record);
            }
            "BEMU" => {
                self.bemu.insert(key.clone(), record);
            }
            other => {
                eprintln!(
                    "warning: skipping runtime bank hash record with unknown source '{other}' in {} line {line_no}",
                    path.display()
                );
                return Ok(());
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
            .whatever_context("failed to flush runtime bank hash compare output")?;
        Ok(self.summary)
    }
}

fn parse_stream_record(
    path: &Path,
    line_no: u64,
    line: &str,
) -> Result<Option<(String, CompareKey, CompareRecord)>, Whatever> {
    let value: Value = serde_json::from_str(line).whatever_context(format!(
        "failed to parse runtime bank hash stream {} line {}",
        path.display(),
        line_no
    ))?;
    let input: CanonicalInput = serde_json::from_value(value).whatever_context(format!(
        "failed to decode runtime bank hash stream {} line {}",
        path.display(),
        line_no
    ))?;

    if input.event_class.as_deref() != Some("bank_data_write") {
        return Ok(None);
    }

    let Some(comparable_seq) = input.comparable_seq else {
        eprintln!(
            "warning: skipping runtime bank hash stream {} line {line_no}: bank_data_write missing comparable_seq",
            path.display()
        );
        return Ok(None);
    };
    let Some(source) = input.source else {
        eprintln!(
            "warning: skipping runtime bank hash stream {} line {line_no}: missing source",
            path.display()
        );
        return Ok(None);
    };

    let key = CompareKey {
        comparable_seq,
        bank_id: input.bank_id,
        version: input.version,
    };
    let record = CompareRecord {
        original_instruction_id: input.original_instruction_id,
        funct7: input.funct7,
        op_type: input.op_type,
        hash: input.hash,
        cycle: input.cycle,
        verilator_time: input.verilator_time,
        pc: input.pc,
        original_record_ref: input
            .original_record_ref
            .or_else(|| Some(format!("{}:{line_no}", path.display()))),
        line_no,
    };

    Ok(Some((source, key, record)))
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
    fn streaming_compare_reports_pass_and_missing() {
        let output = std::env::temp_dir().join(format!("bebop-bank-hash-stream-{}-{}.ndjson", std::process::id(), 1));
        let writer = create_compare_writer(&output).unwrap();
        let mut comparator = StreamingComparator::new(writer, output.clone());

        comparator
            .ingest_line(
                Path::new("stream.ndjson"),
                1,
                r#"{"type":"canonical_bank_hash","source":"RTL","original_instruction_id":4,"comparable_seq":1,"bank_id":0,"version":0,"funct7":33,"op_type":"funct7_33","event_class":"bank_data_write","hash_u64":3746813360834562347,"cycle":10,"pc":2147486388,"original_record_ref":"rtl_bank_hash.ndjson:33","original_log_line":33}"#,
            )
            .unwrap();
        comparator
            .ingest_line(
                Path::new("stream.ndjson"),
                2,
                r#"{"type":"canonical_bank_hash","source":"BEMU","original_instruction_id":2,"comparable_seq":1,"bank_id":0,"version":0,"funct7":33,"op_type":"funct7_33","event_class":"bank_data_write","hash_u64":3746813360834562347,"cycle":2,"pc":2147486388,"original_record_ref":"bemu_bank_hash.ndjson:2","original_log_line":2}"#,
            )
            .unwrap();
        comparator
            .ingest_line(
                Path::new("stream.ndjson"),
                3,
                r#"{"type":"canonical_bank_hash","source":"RTL","original_instruction_id":5,"comparable_seq":2,"bank_id":0,"version":0,"funct7":33,"op_type":"funct7_33","event_class":"bank_data_write","hash_u64":123,"cycle":20,"pc":2147486400,"original_record_ref":"rtl_bank_hash.ndjson:34","original_log_line":34}"#,
            )
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
