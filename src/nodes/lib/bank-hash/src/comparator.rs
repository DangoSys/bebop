use crate::{BTraceBank, BTraceRecord};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct CompareKey {
    pub(crate) hart_id: u64,
    pub(crate) inst_id: u64,
}

impl From<&BTraceRecord> for CompareKey {
    fn from(record: &BTraceRecord) -> Self {
        Self {
            hart_id: record.hart_id,
            inst_id: record.inst_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompareResult {
    Pass,
    Mismatch,
    MissingSubject,
    MissingGolden,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Comparison {
    #[serde(rename = "type")]
    record_type: &'static str,
    pub result: CompareResult,
    pub hart_id: u64,
    pub inst_id: u64,
    pub subject: Option<BTraceBank>,
    pub golden: Option<BTraceBank>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub funct7: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pc: Option<u64>,
}

pub(crate) fn compare(key: CompareKey, rtl: Option<&BTraceRecord>, bemu: Option<&BTraceRecord>) -> Comparison {
    let result = match (rtl, bemu) {
        (Some(rtl), Some(bemu)) if rtl.w0 == bemu.w0 => CompareResult::Pass,
        (Some(_), Some(_)) => CompareResult::Mismatch,
        (None, Some(_)) => CompareResult::MissingSubject,
        (Some(_), None) => CompareResult::MissingGolden,
        (None, None) => unreachable!("comparison key comes from an existing record"),
    };

    Comparison {
        record_type: "btrace_compare",
        result,
        hart_id: key.hart_id,
        inst_id: key.inst_id,
        subject: rtl.map(|record| record.w0),
        golden: bemu.map(|record| record.w0),
        funct7: bemu
            .map(|record| record.funct7)
            .or_else(|| rtl.map(|record| record.funct7)),
        pc: bemu
            .and_then(|record| record.pc)
            .or_else(|| rtl.and_then(|record| record.pc)),
    }
}

fn records_by_key(records: impl IntoIterator<Item = BTraceRecord>) -> BTreeMap<CompareKey, BTraceRecord> {
    let mut keyed = BTreeMap::new();
    for record in records {
        let key = CompareKey::from(&record);
        assert!(
            keyed.insert(key, record).is_none(),
            "duplicate BTrace record: hart={} inst={}",
            key.hart_id,
            key.inst_id
        );
    }
    keyed
}

pub fn compare_offline(
    rtl: impl IntoIterator<Item = BTraceRecord>,
    bemu: impl IntoIterator<Item = BTraceRecord>,
) -> Vec<Comparison> {
    let rtl = records_by_key(rtl);
    let bemu = records_by_key(bemu);
    let keys: BTreeSet<_> = rtl.keys().chain(bemu.keys()).copied().collect();
    keys.into_iter()
        .map(|key| compare(key, rtl.get(&key), bemu.get(&key)))
        .collect()
}
