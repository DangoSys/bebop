use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

mod comparator;

pub use comparator::{
    run_online_with_summary as run_online_compare_with_summary, BankHashCompareSummary, SynchronousBankComparator,
};

const FNV1A_64_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV1A_64_PRIME: u64 = 0x0000_0100_0000_01b3;

pub const BANK_NUM: usize = 32;
pub const BANK_WIDTH: usize = 128;
/// Baseline Toy layout. Runtime DiffTest queries the compiled RTL backdoor
/// directly instead of relying on this value for compatibility checks.
pub const BANK_LINES: usize = 1024;
pub const BANK_SIZE: usize = BANK_LINES * (BANK_WIDTH / 8);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BankHashSource {
    Rtl,
    Bemu,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BankHashRecordType {
    BemuBankHash,
    RtlBankHash,
    CanonicalBankHash,
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
pub enum BankHashPacketId {
    InstructionId(u64),
    RobId(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BankHashTime {
    Cycle(u64),
    VerilatorTime(u64),
}

/// A logical Bank and the state version consumed or produced by an
/// instruction.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BankVersionRef {
    pub bank_id: u32,
    pub version: u32,
}

/// Immutable whole-Bank content captured at an instruction's stable boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BankDigest {
    pub bank_id: u32,
    pub version: u32,
    #[serde(rename = "hash_u64")]
    pub hash: u64,
}

/// Instruction-scoped Bank observation.
///
/// `semantic_seq` is assigned at architectural allocation/issue order and is
/// therefore independent of RTL completion timing. The comparator first
/// validates `actual_banks` against the golden `expected_banks`, then compares
/// the corresponding immutable whole-Bank digests.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InstructionBankBoundaryPacket {
    #[serde(rename = "type")]
    pub record_type: &'static str,
    pub source: BankHashSource,
    pub instruction_id: u64,
    pub semantic_seq: u64,
    pub funct7: u32,
    pub pc: u64,
    pub expected_banks: Vec<u32>,
    pub actual_banks: Vec<u32>,
    pub reads: Vec<BankVersionRef>,
    pub writes: Vec<BankDigest>,
    pub cycle: u64,
    pub cancelled: bool,
}

impl InstructionBankBoundaryPacket {
    pub fn normalize(&mut self) {
        self.expected_banks.sort_unstable();
        self.expected_banks.dedup();
        self.actual_banks.sort_unstable();
        self.actual_banks.dedup();
        self.reads.sort_unstable();
        self.reads.dedup();
        self.writes.sort_unstable_by_key(|entry| (entry.bank_id, entry.version));
    }
}

#[derive(Clone, Debug)]
pub enum RuntimeBankDifftestEvent {
    Hash(CanonicalBankHashPacket),
    Boundary(InstructionBankBoundaryPacket),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BankHashPacket {
    #[serde(rename = "type")]
    pub record_type: BankHashRecordType,
    pub source: BankHashSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instruction_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rob_id: Option<u64>,
    pub bank_id: u32,
    pub version: u32,
    pub op_type: String,
    #[serde(rename = "hash_u64")]
    pub hash: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verilator_time: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CanonicalBankHashPacket {
    #[serde(rename = "type")]
    pub record_type: BankHashRecordType,
    pub source: BankHashSource,
    #[serde(rename = "op_id", alias = "original_instruction_id")]
    pub original_instruction_id: u64,
    pub comparable_seq: Option<u64>,
    pub bank_id: u32,
    #[serde(rename = "bank_version", alias = "version")]
    pub version: u32,
    pub funct7: u32,
    pub op_type: String,
    pub event_class: BankHashEventClass,
    #[serde(rename = "hash_u64")]
    pub hash: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verilator_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pc: Option<u64>,
    pub original_record_ref: String,
    pub original_log_line: u64,
}

impl BankHashPacket {
    pub fn new(
        source: BankHashSource,
        packet_id: BankHashPacketId,
        bank_id: u32,
        op_type: impl Into<String>,
        hash: u64,
        time: BankHashTime,
    ) -> Self {
        let (instruction_id, rob_id) = match packet_id {
            BankHashPacketId::InstructionId(id) => (Some(id), None),
            BankHashPacketId::RobId(id) => (None, Some(id)),
        };
        let (cycle, verilator_time) = match time {
            BankHashTime::Cycle(cycle) => (Some(cycle), None),
            BankHashTime::VerilatorTime(time) => (None, Some(time)),
        };

        Self {
            record_type: match source {
                BankHashSource::Rtl => BankHashRecordType::RtlBankHash,
                BankHashSource::Bemu => BankHashRecordType::BemuBankHash,
            },
            source,
            instruction_id,
            rob_id,
            bank_id,
            version: 0,
            op_type: op_type.into(),
            hash,
            cycle,
            verilator_time,
        }
    }

    pub fn to_ndjson(&self) -> serde_json::Result<String> {
        let mut line = serde_json::to_string(self)?;
        line.push('\n');
        Ok(line)
    }
}

impl CanonicalBankHashPacket {
    pub fn new(
        source: BankHashSource,
        original_instruction_id: u64,
        comparable_seq: Option<u64>,
        bank_id: u32,
        funct7: u32,
        op_type: impl Into<String>,
        event_class: BankHashEventClass,
        hash: u64,
        time: BankHashTime,
        pc: Option<u64>,
        original_record_ref: impl Into<String>,
        original_log_line: u64,
    ) -> Self {
        let (cycle, verilator_time) = match time {
            BankHashTime::Cycle(cycle) => (Some(cycle), None),
            BankHashTime::VerilatorTime(time) => (None, Some(time)),
        };

        Self {
            record_type: BankHashRecordType::CanonicalBankHash,
            source,
            original_instruction_id,
            comparable_seq,
            bank_id,
            version: 0,
            funct7,
            op_type: op_type.into(),
            event_class,
            hash,
            cycle,
            verilator_time,
            pc,
            original_record_ref: original_record_ref.into(),
            original_log_line,
        }
    }

    pub fn with_bank_version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    pub fn to_ndjson(&self) -> serde_json::Result<String> {
        let mut line = serde_json::to_string(self)?;
        line.push('\n');
        Ok(line)
    }
}

static RUNTIME_PACKET_SINK: OnceLock<Mutex<Option<Sender<RuntimeBankDifftestEvent>>>> = OnceLock::new();
static RUNTIME_DIFFTEST_FAILURE_DETECTED: AtomicBool = AtomicBool::new(false);

fn get_runtime_packet_sink() -> &'static Mutex<Option<Sender<RuntimeBankDifftestEvent>>> {
    RUNTIME_PACKET_SINK.get_or_init(|| Mutex::new(None))
}

pub fn init_runtime_packet_channel() -> Receiver<RuntimeBankDifftestEvent> {
    RUNTIME_DIFFTEST_FAILURE_DETECTED.store(false, Ordering::Release);
    let (sender, receiver) = mpsc::channel::<RuntimeBankDifftestEvent>();
    *get_runtime_packet_sink().lock().unwrap() = Some(sender);
    receiver
}

pub fn runtime_bank_difftest_failure_detected() -> bool {
    RUNTIME_DIFFTEST_FAILURE_DETECTED.load(Ordering::Acquire)
}

/// Request that the running RTL simulation stop at its next DiffTest polling
/// boundary. This is shared by hash mismatches, Bank-target mismatches, and
/// write-attribution failures.
pub fn report_runtime_bank_difftest_failure() {
    RUNTIME_DIFFTEST_FAILURE_DETECTED.store(true, Ordering::Release);
}

pub fn submit_runtime_bank_hash_packet(packet: CanonicalBankHashPacket) {
    let sink = get_runtime_packet_sink().lock().unwrap().clone();
    if let Some(sink) = sink {
        let _ = sink.send(RuntimeBankDifftestEvent::Hash(packet));
    }
}

pub fn submit_runtime_bank_boundary(mut packet: InstructionBankBoundaryPacket) {
    packet.normalize();
    let sink = get_runtime_packet_sink().lock().unwrap().clone();
    if let Some(sink) = sink {
        let _ = sink.send(RuntimeBankDifftestEvent::Boundary(packet));
    }
}

pub fn shutdown_runtime_packet_channel() {
    get_runtime_packet_sink().lock().unwrap().take();
}

pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV1A_64_OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV1A_64_PRIME)
    })
}

pub fn bank_hash(bytes: &[u8]) -> u64 {
    fnv1a_64(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn same_bytes_hash_to_same_value() {
        let bytes = b"bebop-bank-hash";

        assert_eq!(bank_hash(bytes), bank_hash(bytes));
        assert_eq!(bank_hash(bytes), fnv1a_64(bytes));
    }

    #[test]
    fn changing_one_byte_changes_hash() {
        let before = b"bebop-bank-hash";
        let mut after = *before;
        after[0] ^= 0x01;

        assert_ne!(bank_hash(before), bank_hash(&after));
    }

    #[test]
    fn fnv1a_64_matches_known_vectors() {
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_64(b"hello"), 0xa430_d846_80aa_bd0b);
    }

    #[test]
    fn bank_hash_packet_serializes_to_ndjson_log_line() {
        let packet = BankHashPacket::new(
            BankHashSource::Bemu,
            BankHashPacketId::InstructionId(42),
            7,
            "mset",
            bank_hash(b"payload"),
            BankHashTime::Cycle(1234),
        );

        let line = packet.to_ndjson().expect("packet should serialize");
        assert!(line.ends_with('\n'));

        let value: Value = serde_json::from_str(line.trim_end()).expect("line should be valid JSON");
        assert_eq!(value["type"], "bemu_bank_hash");
        assert_eq!(value["source"], "BEMU");
        assert_eq!(value["instruction_id"], 42);
        assert!(value.get("rob_id").is_none());
        assert_eq!(value["bank_id"], 7);
        assert_eq!(value["version"], 0);
        assert_eq!(value["op_type"], "mset");
        assert_eq!(value["hash_u64"], packet.hash);
        assert_eq!(value["cycle"], 1234);
        assert!(value.get("verilator_time").is_none());
    }

    #[test]
    fn rtl_bank_hash_packet_serializes_to_rtl_log_line() {
        let packet = BankHashPacket::new(
            BankHashSource::Rtl,
            BankHashPacketId::InstructionId(99),
            3,
            "funct7_33",
            bank_hash(b"rtl-payload"),
            BankHashTime::Cycle(456),
        );

        let value: Value =
            serde_json::from_str(packet.to_ndjson().expect("packet should serialize").trim_end()).unwrap();
        assert_eq!(value["type"], "rtl_bank_hash");
        assert_eq!(value["source"], "RTL");
        assert_eq!(value["instruction_id"], 99);
        assert_eq!(value["bank_id"], 3);
        assert_eq!(value["hash_u64"], packet.hash);
        assert_eq!(value["cycle"], 456);
    }

    #[test]
    fn canonical_bank_hash_packet_serializes_to_ndjson_log_line() {
        let packet = CanonicalBankHashPacket::new(
            BankHashSource::Rtl,
            4,
            Some(1),
            0,
            33,
            "funct7_33",
            BankHashEventClass::BankDataWrite,
            3746813360834562347,
            BankHashTime::Cycle(13426248),
            Some(0x8000_0ab4),
            "rtl_bank_hash.ndjson:33",
            33,
        );

        let value: Value =
            serde_json::from_str(packet.to_ndjson().expect("packet should serialize").trim_end()).unwrap();
        assert_eq!(value["type"], "canonical_bank_hash");
        assert_eq!(value["source"], "RTL");
        assert_eq!(value["op_id"], 4);
        assert_eq!(value["comparable_seq"], 1);
        assert_eq!(value["bank_id"], 0);
        assert_eq!(value["bank_version"], 0);
        assert_eq!(value["funct7"], 33);
        assert_eq!(value["op_type"], "funct7_33");
        assert_eq!(value["event_class"], "bank_data_write");
        assert_eq!(value["hash_u64"], 3746813360834562347u64);
        assert_eq!(value["cycle"], 13426248);
        assert_eq!(value["pc"], 0x8000_0ab4u64);
        assert_eq!(value["original_record_ref"], "rtl_bank_hash.ndjson:33");
        assert_eq!(value["original_log_line"], 33);
    }
}
