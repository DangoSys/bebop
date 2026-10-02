use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock,
};
use std::time::Instant;

pub const RECORD_BYTES: u64 = 72;
static ENABLED: AtomicBool = AtomicBool::new(false);
static SESSION: OnceLock<Mutex<Option<Comparator>>> = OnceLock::new();

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WriteRecord {
    pub stream_hart: u64,
    pub hart: u64,
    pub inst: u64,
    pub shared: u32,
    pub bank: u32,
    pub group: u32,
    pub physical: u32,
    pub sequence: u64,
    pub addr: u32,
    pub mask: u32,
    pub data: [u8; 16],
}

type Key = (u64, u64, u32, u32, u32, u32);
#[derive(Default)]
pub struct Comparator {
    subject: BTreeMap<Key, VecDeque<WriteRecord>>,
    golden: BTreeMap<Key, VecDeque<WriteRecord>>,
    pub received: BTreeMap<(u64, u32, u32), u64>,
    pub subject_count: u64,
    pub golden_count: u64,
    pub matched: u64,
    pub comparison_time_s: f64,
    pub failure: Option<String>,
}

impl Comparator {
    pub fn observe(&mut self, rtl: bool, record: WriteRecord) {
        if self.failure.is_some() {
            return;
        }
        if record.mask & !0xffff != 0 {
            self.failure = Some(format!("invalid write mask: {record:?}"));
            return;
        }
        if rtl {
            let next = self
                .received
                .entry((record.stream_hart, record.shared, record.physical))
                .or_default();
            if record.sequence != *next {
                self.failure = Some(format!(
                    "lost/duplicate write: expected sequence {next}, got {record:?}"
                ));
                return;
            }
            *next += 1;
            self.subject_count += 1;
        } else {
            self.golden_count += 1;
        }
        let key = (
            record.hart,
            record.inst,
            record.shared,
            record.bank,
            record.group,
            record.addr,
        );
        let (own, other) = if rtl {
            (&mut self.subject, &mut self.golden)
        } else {
            (&mut self.golden, &mut self.subject)
        };
        if let Some(queue) = other.get_mut(&key) {
            let expected = queue.pop_front().unwrap();
            if queue.is_empty() {
                other.remove(&key);
            }
            let started = Instant::now();
            let mismatch = record.mask != expected.mask
                || (0..16).any(|i| record.mask & (1 << i) != 0 && record.data[i] != expected.data[i]);
            self.comparison_time_s += started.elapsed().as_secs_f64();
            if mismatch {
                self.failure = Some(format!("SPM write mismatch: {record:?}; counterpart={expected:?}"));
            } else {
                self.matched += 1;
            }
        } else {
            own.entry(key).or_default().push_back(record);
        }
    }

    pub fn finish(&self) -> Result<(), String> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if !self.subject.is_empty() || !self.golden.is_empty() {
            return Err(format!(
                "unmatched SPM writes: rtl={} reference={} matched={}; first RTL={:?}; first reference={:?}",
                self.subject_count,
                self.golden_count,
                self.matched,
                self.subject.first_key_value(),
                self.golden.first_key_value()
            ));
        }
        Ok(())
    }
}

fn session() -> &'static Mutex<Option<Comparator>> {
    SESSION.get_or_init(|| Mutex::new(None))
}
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}
pub fn start() {
    let mut slot = session().lock().unwrap();
    assert!(slot.is_none(), "access session already active");
    *slot = Some(Comparator::default());
    ENABLED.store(true, Ordering::Relaxed);
}
pub fn observe(rtl: bool, record: WriteRecord) {
    session()
        .lock()
        .unwrap()
        .as_mut()
        .expect("access session is active")
        .observe(rtl, record);
}
pub fn inspect<R>(f: impl FnOnce(&Comparator) -> R) -> R {
    f(session().lock().unwrap().as_ref().expect("access session is active"))
}
pub fn subject_matched() -> bool {
    inspect(|s| s.subject.is_empty())
}
pub fn failure() -> Option<String> {
    inspect(|s| s.failure.clone())
}
pub fn finish() -> Result<(), String> {
    ENABLED.store(false, Ordering::Relaxed);
    session()
        .lock()
        .unwrap()
        .take()
        .expect("access session is active")
        .finish()
}
pub fn cancel() {
    ENABLED.store(false, Ordering::Relaxed);
    session().lock().unwrap().take();
}
