use serde::{Deserialize, Serialize};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use xxhash_rust::xxh64::xxh64;

mod comparator;

pub use comparator::{
    compare_offline, run_online_with_summary as run_online_compare_with_summary, BankDigestCompareResult,
    BankDigestComparison, BankHashCompareSummary,
};

/// DiffTest-N uses XXH64 with a fixed seed on both sides of the interface.
pub const BANK_DIGEST_SEED: u64 = 0;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BankHashSource {
    Rtl,
    Bemu,
}

/// Architectural bank identity shared by BEMU and RTL.
///
/// Physical SRAM slots are intentionally excluded: an mset operation may bind
/// the same virtual bank group to a different physical slot on either side.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct LogicalBankId {
    pub vbank_id: u32,
    pub group_id: u32,
}

impl LogicalBankId {
    pub const fn new(vbank_id: u32, group_id: u32) -> Self {
        Self { vbank_id, group_id }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BankHashRecordType {
    BankDigest,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BankHashEventClass {
    BootInit,
    ControlOnly,
    ConfigOnly,
    MemoryOnly,
    BankDataWrite,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BankHashTime {
    Cycle(u64),
    VerilatorTime(u64),
}

/// comparison record: <InstID, LogicalBankID, Digest>.
///
/// The remaining fields are diagnostic metadata and never participate in
/// record alignment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BankDigestRecord {
    #[serde(rename = "type")]
    pub record_type: BankHashRecordType,
    pub source: BankHashSource,
    pub instruction_id: u64,
    pub bank_id: LogicalBankId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical_bank_id: Option<u32>,
    #[serde(rename = "digest_u64")]
    pub digest: u64,
    pub funct7: u32,
    pub op_type: String,
    pub event_class: BankHashEventClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verilator_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pc: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_record_ref: Option<String>,
}

impl BankDigestRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: BankHashSource,
        instruction_id: u64,
        bank_id: LogicalBankId,
        physical_bank_id: Option<u32>,
        digest: u64,
        funct7: u32,
        op_type: impl Into<String>,
        event_class: BankHashEventClass,
        time: BankHashTime,
        pc: Option<u64>,
        original_record_ref: Option<String>,
    ) -> Self {
        let (cycle, verilator_time) = match time {
            BankHashTime::Cycle(cycle) => (Some(cycle), None),
            BankHashTime::VerilatorTime(time) => (None, Some(time)),
        };

        Self {
            record_type: BankHashRecordType::BankDigest,
            source,
            instruction_id,
            bank_id,
            physical_bank_id,
            digest,
            funct7,
            op_type: op_type.into(),
            event_class,
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

static RUNTIME_PACKET_SINK: OnceLock<Mutex<Option<Sender<BankDigestRecord>>>> = OnceLock::new();

fn get_runtime_packet_sink() -> &'static Mutex<Option<Sender<BankDigestRecord>>> {
    RUNTIME_PACKET_SINK.get_or_init(|| Mutex::new(None))
}

pub fn init_runtime_packet_channel() -> Receiver<BankDigestRecord> {
    let (sender, receiver) = mpsc::channel::<BankDigestRecord>();
    *get_runtime_packet_sink().lock().unwrap() = Some(sender);
    receiver
}

pub fn submit_runtime_bank_digest(record: &BankDigestRecord) {
    if let Some(sink) = get_runtime_packet_sink().lock().unwrap().as_ref() {
        sink.send(record.clone()).ok();
    }
}

pub fn shutdown_runtime_packet_channel() {
    get_runtime_packet_sink().lock().unwrap().take();
}

/// Hashes the complete canonical bank byte sequence using XXH64(seed = 0).
pub fn bank_hash(bytes: &[u8]) -> u64 {
    xxh64(bytes, BANK_DIGEST_SEED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn xxh64_matches_known_vectors() {
        assert_eq!(bank_hash(b""), 0xef46_db37_51d8_e999);
        assert_eq!(bank_hash(b"hello"), 0x26c7_827d_889f_6da3);
    }

    #[test]
    fn changing_one_byte_changes_digest() {
        let before = b"bebop-bank-hash";
        let mut after = *before;
        after[0] ^= 0x01;

        assert_ne!(bank_hash(before), bank_hash(&after));
    }

    #[test]
    fn bank_digest_record_serializes_canonical_identity() {
        let record = BankDigestRecord::new(
            BankHashSource::Bemu,
            42,
            LogicalBankId::new(7, 2),
            Some(11),
            bank_hash(b"payload"),
            64,
            "funct7_64",
            BankHashEventClass::BankDataWrite,
            BankHashTime::Cycle(1234),
            Some(0x8000_1000),
            Some("bemu_bank_digest.ndjson:1".into()),
        );

        let line = record.to_ndjson().expect("record should serialize");
        assert!(line.ends_with('\n'));
        let value: Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(value["type"], "bank_digest");
        assert_eq!(value["source"], "BEMU");
        assert_eq!(value["instruction_id"], 42);
        assert_eq!(value["bank_id"]["vbank_id"], 7);
        assert_eq!(value["bank_id"]["group_id"], 2);
        assert_eq!(value["physical_bank_id"], 11);
        assert_eq!(value["digest_u64"], record.digest);
    }
}
