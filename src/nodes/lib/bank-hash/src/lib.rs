use serde::{Deserialize, Serialize};
use snafu::{FromString, ResultExt, Whatever};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

mod comparator;

pub use comparator::{compare_offline, CompareResult, Comparison};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BTraceSource {
    Rtl,
    Bemu,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BTraceBank {
    pub vbank_id: u32,
    #[serde(rename = "hash_u32")]
    pub hash: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BTraceTime {
    Cycle(u64),
    VerilatorTime(u64),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BTraceRecord {
    #[serde(rename = "type")]
    pub record_type: String,
    pub source: BTraceSource,
    pub inst_id: u64,
    pub hart_id: u64,
    pub w0: BTraceBank,
    pub funct7: u32,
    pub op_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verilator_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pc: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_record_ref: Option<String>,
}

impl BTraceRecord {
    pub fn new(
        source: BTraceSource,
        inst_id: u64,
        hart_id: u64,
        w0: BTraceBank,
        funct7: u32,
        op_type: impl Into<String>,
        time: BTraceTime,
        pc: Option<u64>,
        original_record_ref: Option<String>,
    ) -> Self {
        let (cycle, verilator_time) = match time {
            BTraceTime::Cycle(cycle) => (Some(cycle), None),
            BTraceTime::VerilatorTime(time) => (None, Some(time)),
        };

        Self {
            record_type: "btrace".to_string(),
            source,
            inst_id,
            hart_id,
            w0,
            funct7,
            op_type: op_type.into(),
            cycle,
            verilator_time,
            pc,
            original_record_ref,
        }
    }

    pub fn to_ndjson(&self) -> serde_json::Result<String> {
        let mut line = serde_json::to_string(self)?;
        line.push('\n');
        Ok(line)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Progress {
    pub subject: u64,
    pub golden: u64,
}

struct Session {
    subject: BTreeMap<comparator::CompareKey, BTraceRecord>,
    golden: BTreeMap<comparator::CompareKey, BTraceRecord>,
    compared: BTreeSet<comparator::CompareKey>,
    writer: BufWriter<File>,
    output: PathBuf,
    progress: Progress,
    failure: Option<String>,
}

static SESSION: OnceLock<Mutex<Option<Session>>> = OnceLock::new();

fn session() -> &'static Mutex<Option<Session>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

pub fn start(output: PathBuf) -> Result<(), Whatever> {
    let mut slot = session().lock().unwrap();
    assert!(slot.is_none(), "DiffTest session is already active");
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .whatever_context(format!("failed to create output directory {}", parent.display()))?;
    }
    let writer = File::create(&output)
        .map(BufWriter::new)
        .whatever_context(format!("failed to create {}", output.display()))?;
    *slot = Some(Session {
        subject: BTreeMap::new(),
        golden: BTreeMap::new(),
        compared: BTreeSet::new(),
        writer,
        output,
        progress: Progress::default(),
        failure: None,
    });
    Ok(())
}

pub fn observe(record: &BTraceRecord) {
    let mut slot = session().lock().unwrap();
    let Some(state) = slot.as_mut() else {
        return;
    };
    let key = comparator::CompareKey::from(record);
    if state.compared.contains(&key) {
        state.failure = Some("duplicate BTrace record after comparison".to_string());
        return;
    }
    let pair = match record.source {
        BTraceSource::Rtl => {
            state.progress.subject += 1;
            if let Some(golden) = state.golden.remove(&key) {
                Some((record.clone(), golden))
            } else {
                if state.subject.insert(key, record.clone()).is_some() {
                    state.failure = Some("duplicate subject BTrace record".to_string());
                }
                None
            }
        }
        BTraceSource::Bemu => {
            state.progress.golden += 1;
            if let Some(subject) = state.subject.remove(&key) {
                Some((subject, record.clone()))
            } else {
                if state.golden.insert(key, record.clone()).is_some() {
                    state.failure = Some("duplicate golden BTrace record".to_string());
                }
                None
            }
        }
    };
    if let Some((subject, golden)) = pair {
        let comparison = comparator::compare(key, Some(&subject), Some(&golden));
        if let Err(error) = write(&mut state.writer, &state.output, &comparison) {
            state.failure = Some(error.to_string());
            return;
        }
        state.compared.insert(key);
        if comparison.result == CompareResult::Mismatch {
            state.failure = Some(format!(
                "Bank DiffTest mismatch: hart={} inst={}",
                key.hart_id, key.inst_id
            ));
        }
    }
}

pub fn progress() -> Progress {
    session()
        .lock()
        .unwrap()
        .as_ref()
        .expect("DiffTest session is active")
        .progress
}

pub fn subject_matched() -> bool {
    session()
        .lock()
        .unwrap()
        .as_ref()
        .expect("DiffTest session is active")
        .subject
        .is_empty()
}

pub fn failure() -> Option<String> {
    session()
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|state| state.failure.clone())
}

pub fn finish() -> Result<(), Whatever> {
    let mut state = session().lock().unwrap().take().expect("DiffTest session is active");
    if let Some(message) = state.failure {
        return Err(Whatever::without_source(message));
    }
    let remaining = compare_offline(state.subject.into_values(), state.golden.into_values());
    for comparison in &remaining {
        write(&mut state.writer, &state.output, comparison)?;
    }
    state
        .writer
        .flush()
        .whatever_context("failed to flush bank hash comparison output")?;
    if let Some(comparison) = remaining.first() {
        return Err(Whatever::without_source(format!(
            "unmatched BTrace record: hart={} inst={}",
            comparison.hart_id, comparison.inst_id
        )));
    }
    Ok(())
}

pub fn cancel() {
    session().lock().unwrap().take();
}

fn write(writer: &mut BufWriter<File>, output: &PathBuf, comparison: &Comparison) -> Result<(), Whatever> {
    serde_json::to_writer(&mut *writer, comparison)
        .whatever_context(format!("failed to write {}", output.display()))?;
    writer
        .write_all(b"\n")
        .whatever_context(format!("failed to write {}", output.display()))?;
    writer
        .flush()
        .whatever_context(format!("failed to flush {}", output.display()))?;
    Ok(())
}

fn rotate_left(value: u32, amount: u32) -> u32 {
    value.rotate_left(amount)
}

pub fn bank_row_hash(addr: u32, row: &[u8]) -> u32 {
    assert_eq!(row.len(), 16, "bank hash requires 16-byte rows");
    let word0 = u32::from_le_bytes(row[..4].try_into().expect("row word 0"));
    let word1 = u32::from_le_bytes(row[4..8].try_into().expect("row word 1"));
    let word2 = u32::from_le_bytes(row[8..12].try_into().expect("row word 2"));
    let word3 = u32::from_le_bytes(row[12..].try_into().expect("row word 3"));
    if word0 == 0 && word1 == 0 && word2 == 0 && word3 == 0 {
        return 0;
    }
    word0 ^ rotate_left(word1, 7) ^ rotate_left(word2, 13) ^ rotate_left(word3, 21) ^ rotate_left(addr, 11)
}

pub fn bank_hash(bytes: &[u8], row_bytes: usize) -> u32 {
    assert_eq!(row_bytes, 16, "bank hash requires 16-byte rows");
    assert_eq!(bytes.len() % row_bytes, 0, "bank must contain complete rows");
    bytes
        .chunks_exact(row_bytes)
        .enumerate()
        .fold(0u32, |hash, (addr, row)| {
            hash.wrapping_add(bank_row_hash(
                u32::try_from(addr).expect("bank row address exceeds u32"),
                row,
            ))
        })
}

pub fn combine_bank_hash(status_hash: u32, group_id: u32, physical_hash: u32) -> u32 {
    let mixed = physical_hash ^ group_id.rotate_left(7) ^ 0x9e37_79b9;
    status_hash.rotate_left(5) ^ mixed ^ mixed.rotate_left(13)
}
