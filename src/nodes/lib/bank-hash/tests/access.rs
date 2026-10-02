use bebop_bank_hash::access::{Comparator, WriteRecord, RECORD_BYTES};
fn event(sequence: u64, addr: u32, value: u8) -> WriteRecord {
    WriteRecord {
        stream_hart: 0,
        hart: 0,
        inst: 7,
        shared: 0,
        bank: 2,
        group: 0,
        physical: 1,
        sequence,
        addr,
        mask: 0xffff,
        data: [value; 16],
    }
}
#[test]
fn repeated_writes_and_independent_banks() {
    let mut c = Comparator::default();
    let a = event(0, 4, 9);
    let mut b = event(0, 4, 3);
    b.bank = 3;
    b.physical = 2;
    for r in [&a, &b, &event(1, 4, 10)] {
        c.observe(true, r.clone());
    }
    for mut r in [b, a, event(1, 4, 10)] {
        r.physical = 99;
        c.observe(false, r);
    }
    c.finish().unwrap();
    assert_eq!(c.matched, 3);
    assert_eq!(RECORD_BYTES, 18 * 4);
}
#[test]
fn masks_only_ignore_unwritten_lanes() {
    let mut c = Comparator::default();
    let mut r = event(0, 0, 1);
    r.mask = 1;
    c.observe(true, r.clone());
    r.data[15] = 99;
    c.observe(false, r);
    c.finish().unwrap();
}
#[test]
fn every_semantic_field_is_checked() {
    for field in 0..7 {
        let mut c = Comparator::default();
        c.observe(true, event(0, 0, 1));
        let mut r = event(0, 0, 1);
        match field {
            0 => r.data[0] = 2,
            1 => r.mask = 1,
            2 => r.addr = 1,
            3 => r.inst += 1,
            4 => r.bank += 1,
            5 => r.group += 1,
            _ => r.hart += 1,
        }
        c.observe(false, r);
        assert!(c.finish().is_err(), "field {field}");
    }
}
#[test]
fn intermediate_corruption_is_not_hidden_by_last_write() {
    let mut c = Comparator::default();
    c.observe(true, event(0, 0, 8));
    c.observe(true, event(1, 0, 1));
    c.observe(false, event(0, 0, 9));
    c.observe(false, event(1, 0, 1));
    assert!(c.finish().is_err());
}
#[test]
fn missing_duplicate_and_trailing_events_fail() {
    let mut c = Comparator::default();
    c.observe(true, event(1, 0, 1));
    assert!(c.finish().is_err());
    let mut c = Comparator::default();
    c.observe(true, event(0, 0, 1));
    c.observe(false, event(0, 0, 1));
    c.observe(true, event(0, 0, 1));
    assert!(c.finish().is_err());
    for rtl in [true, false] {
        let mut c = Comparator::default();
        c.observe(rtl, event(0, 0, 1));
        assert!(c.finish().is_err());
    }
}
