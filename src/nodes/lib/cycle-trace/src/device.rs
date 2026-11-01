use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

type Identity = (u64, u64, u64, u64, u64);

#[derive(Default)]
struct Packet {
    words: [u64; 12],
    next: usize,
}

#[derive(Default)]
struct Summary {
    first: Option<u64>,
    last: u64,
    sum: u64,
    count: u64,
}

pub struct CycleTrace {
    directory: PathBuf,
    packets: HashMap<u32, Packet>,
    starts: HashMap<Identity, [u64; 12]>,
    summaries: HashMap<(u64, u64), Summary>,
    written: HashSet<PathBuf>,
}

impl CycleTrace {
    pub fn new(log_dir: &Path) -> Self {
        Self {
            directory: log_dir.join("trace"),
            packets: HashMap::new(),
            starts: HashMap::new(),
            summaries: HashMap::new(),
            written: HashSet::new(),
        }
    }

    pub fn write(&mut self, channel: u32, word: usize, value: u64) -> Result<(), String> {
        let packet = self.packets.entry(channel).or_default();
        if word != packet.next || word >= 12 {
            return Err(format!(
                "trace channel {channel}: expected word {}, got {word}",
                packet.next
            ));
        }
        packet.words[word] = value;
        packet.next += 1;
        if packet.next != 12 {
            return Ok(());
        }
        packet.next = 0;
        let event = packet.words;
        let [kind, id, controller, thread, pid, tid, depth, _, _, _, _, cycle] = event;
        if !(1..=4).contains(&depth) || event[7..7 + depth as usize].iter().any(|v| *v > i64::MAX as u64) {
            return Err("invalid trace path".into());
        }
        let identity = (controller, thread, pid, tid, id);
        match kind {
            1 => {
                if self.starts.insert(identity, event).is_some() {
                    return Err(format!("trace {identity:?} already started"));
                }
            }
            2 => {
                let start = self
                    .starts
                    .remove(&identity)
                    .ok_or_else(|| format!("trace {identity:?} has no start"))?;
                if event[6..11] != start[6..11] || cycle < start[11] {
                    return Err(format!("trace {identity:?} has mismatched end"));
                }
                let directory = self
                    .directory
                    .join(format!("controller-{controller}/thread-{thread}/cycle"));
                fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                let path = event[7..7 + depth as usize]
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("-");
                let file_path = directory.join(format!("trace-{path}.txt"));
                let first_record = self.written.insert(file_path.clone());
                let mut file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(first_record)
                    .append(!first_record)
                    .open(file_path)
                    .map_err(|e| e.to_string())?;
                let elapsed = cycle - start[11];
                writeln!(
                    file,
                    "pid {pid}\ntid {tid}\nstart {}\nend {cycle}\nelapsed {elapsed}",
                    start[11]
                )
                .map_err(|e| e.to_string())?;
                let summary = self.summaries.entry((controller, thread)).or_default();
                summary.first = Some(summary.first.map_or(start[11], |old| old.min(start[11])));
                summary.last = summary.last.max(cycle);
                summary.count += 1;
                if depth == 1 {
                    summary.sum += elapsed;
                }
                let first = summary.first.unwrap();
                fs::write(
                    directory.join("summary.txt"),
                    format!(
                        "first_start {first}\nlast_end {}\ntrace_span {}\ntraced_cycle_sum {}\ntrace_count {}\n",
                        summary.last,
                        summary.last - first,
                        summary.sum,
                        summary.count,
                    ),
                )
                .map_err(|e| e.to_string())?;
            }
            _ => return Err(format!("unknown trace event {kind}")),
        }
        Ok(())
    }

    pub fn finish(&self) -> Result<(), String> {
        if !self.starts.is_empty() || self.packets.values().any(|packet| packet.next != 0) {
            return Err("unfinished trace events".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_callers_and_complete_records() {
        let directory = std::env::temp_dir().join(format!("trace-device-{}", std::process::id()));
        let mut trace = CycleTrace::new(&directory);
        for event in [
            [1, 7, 0, 0, 10, 11, 1, 7, u64::MAX, u64::MAX, u64::MAX, 100],
            [1, 7, 0, 0, 20, 21, 1, 7, u64::MAX, u64::MAX, u64::MAX, 110],
            [2, 7, 0, 0, 20, 21, 1, 7, u64::MAX, u64::MAX, u64::MAX, 130],
            [2, 7, 0, 0, 10, 11, 1, 7, u64::MAX, u64::MAX, u64::MAX, 140],
        ] {
            for (word, value) in event.into_iter().enumerate() {
                trace.write(0, word, value).unwrap();
            }
        }
        trace.finish().unwrap();
        let summary = fs::read_to_string(directory.join("trace/controller-0/thread-0/cycle/summary.txt")).unwrap();
        assert!(summary.contains("traced_cycle_sum 60\ntrace_count 2\n"));
        let records = fs::read_to_string(directory.join("trace/controller-0/thread-0/cycle/trace-7.txt")).unwrap();
        assert!(records.contains("pid 20\ntid 21\nstart 110\nend 130\nelapsed 20"));
        assert!(records.contains("pid 10\ntid 11\nstart 100\nend 140\nelapsed 40"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_incomplete_and_out_of_order_packets() {
        let mut trace = CycleTrace::new(Path::new("unused"));
        assert!(trace.write(0, 1, 0).is_err());
        trace.write(0, 0, 1).unwrap();
        assert!(trace.finish().is_err());
    }
}
