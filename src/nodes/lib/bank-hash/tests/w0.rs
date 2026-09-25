use bebop_bank_hash::{compare_offline, BTraceBank, BTraceRecord, BTraceSource, BTraceTime, CompareResult};

fn record(source: BTraceSource, hart_id: u64, inst_id: u64, vbank_id: u32, hash: u32) -> BTraceRecord {
    BTraceRecord::new(
        source,
        inst_id,
        hart_id,
        BTraceBank { vbank_id, hash },
        33,
        "mvin",
        BTraceTime::Cycle(1),
        None,
        None,
    )
}

#[test]
fn compares_only_the_matching_hart_and_instruction() {
    let subject = [
        record(BTraceSource::Rtl, 0, 7, 4, 11),
        record(BTraceSource::Rtl, 1, 7, 5, 12),
    ];
    let golden = [
        record(BTraceSource::Bemu, 1, 7, 5, 12),
        record(BTraceSource::Bemu, 0, 7, 4, 11),
    ];
    let comparisons = compare_offline(subject, golden);
    assert_eq!(comparisons.len(), 2);
    assert!(comparisons
        .iter()
        .all(|comparison| comparison.result == CompareResult::Pass));
    let diff = serde_json::to_value(&comparisons[0]).unwrap();
    assert!(diff["subject"].is_object());
    assert!(diff["golden"].is_object());
}

#[test]
fn detects_wrong_hash_bank_and_unmatched_records() {
    let subject = [
        record(BTraceSource::Rtl, 0, 1, 4, 11),
        record(BTraceSource::Rtl, 0, 2, 4, 11),
        record(BTraceSource::Rtl, 0, 3, 4, 11),
        record(BTraceSource::Rtl, 0, 4, 4, 11),
    ];
    let golden = [
        record(BTraceSource::Bemu, 0, 1, 4, 12),
        record(BTraceSource::Bemu, 0, 2, 5, 11),
        record(BTraceSource::Bemu, 0, 5, 4, 11),
    ];
    let comparisons = compare_offline(subject, golden);
    assert_eq!(
        comparisons.iter().map(|item| item.result).collect::<Vec<_>>(),
        [
            CompareResult::Mismatch,
            CompareResult::Mismatch,
            CompareResult::MissingGolden,
            CompareResult::MissingGolden,
            CompareResult::MissingSubject,
        ]
    );
}

#[test]
#[should_panic(expected = "duplicate BTrace record")]
fn rejects_duplicate_records() {
    compare_offline(
        [
            record(BTraceSource::Rtl, 0, 7, 4, 11),
            record(BTraceSource::Rtl, 0, 7, 4, 12),
        ],
        [],
    );
}

#[test]
fn rejects_legacy_three_bank_record() {
    let mut value = serde_json::to_value(record(BTraceSource::Rtl, 0, 7, 4, 11)).unwrap();
    assert_eq!(value["hart_id"], 0);
    assert_eq!(value["inst_id"], 7);
    assert_eq!(value["w0"], serde_json::json!({"vbank_id": 4, "hash_u32": 11}));
    assert!(value.get("r0").is_none());
    assert!(value.get("r1").is_none());
    value["r0"] = serde_json::json!({"vbank_id": 4294967295u32, "hash_u32": 0});
    assert!(serde_json::from_value::<BTraceRecord>(value).is_err());
}
