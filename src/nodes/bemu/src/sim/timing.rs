use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Residency {
    Absent,
    Clean,
    Dirty,
}

pub(crate) struct CpuCache {
    tags: Vec<AtomicU64>,
}

impl CpuCache {
    pub(crate) fn new(lines: usize) -> Self {
        assert!(lines >= 2 && lines.is_power_of_two());
        Self {
            tags: (0..lines).map(|_| AtomicU64::new(u64::MAX)).collect(),
        }
    }

    pub(crate) fn access(&self, address: u64, write: bool) {
        let line = address >> 6;
        let slot = &self.tags[line as usize & (self.tags.len() - 1)];
        let old = slot.load(Ordering::Relaxed);
        let dirty = write || (old >> 1 == line && old & 1 != 0);
        let new = (line << 1) | u64::from(dirty);
        if write || new != old {
            slot.store(new, Ordering::Release);
        }
    }

    fn state(&self, line: u64) -> Residency {
        let entry = self.tags[line as usize & (self.tags.len() - 1)].load(Ordering::Acquire);
        if entry >> 1 != line {
            Residency::Absent
        } else if entry & 1 != 0 {
            Residency::Dirty
        } else {
            Residency::Clean
        }
    }

    fn maintain(&self, line: u64, invalidate: bool) -> Residency {
        let slot = &self.tags[line as usize & (self.tags.len() - 1)];
        let mut entry = slot.load(Ordering::Acquire);
        loop {
            if entry >> 1 != line {
                return Residency::Absent;
            }
            let state = if entry & 1 != 0 {
                Residency::Dirty
            } else {
                Residency::Clean
            };
            let new = if invalidate { u64::MAX } else { entry & !1 };
            if new == entry {
                return state;
            }
            match slot.compare_exchange_weak(entry, new, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return state,
                Err(current) => entry = current,
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct Calendar(BTreeMap<u64, u64>);

impl Calendar {
    pub(crate) fn reserve(&mut self, mut start: u64, duration: u64, minimum: u64) -> u64 {
        while self.0.first_key_value().is_some_and(|(_, &end)| end <= minimum) {
            self.0.pop_first();
        }
        if let Some((_, &end)) = self.0.range(..=start).next_back() {
            start = start.max(end);
        }
        for (&next, &end) in self.0.range(start..) {
            if start + duration <= next {
                break;
            }
            start = end;
        }
        self.0.insert(start, start + duration);
        start + duration
    }
}

pub(crate) struct DeviceTime {
    pub(crate) threads: OnceLock<Vec<Arc<AtomicU64>>>,
    pub(crate) cpus: OnceLock<Vec<Arc<CpuCache>>>,
    clint: Arc<crate::device::clint::Clint>,
    ddr: Mutex<Calendar>,
}

impl DeviceTime {
    pub(crate) fn new(clint: Arc<crate::device::clint::Clint>) -> Self {
        Self {
            threads: OnceLock::new(),
            cpus: OnceLock::new(),
            clint,
            ddr: Mutex::new(Calendar::default()),
        }
    }

    fn minimum(&self) -> u64 {
        self.threads
            .get()
            .expect("time participants configured")
            .iter()
            .fold(self.clint.cycles(), |time, thread| {
                time.min(thread.load(Ordering::Acquire))
            })
    }
}

pub(crate) struct CoreTime {
    device: Arc<DeviceTime>,
    maintenance: Calendar,
    transport: Calendar,
    cache: Option<Vec<Option<(u64, bool)>>>,
}

impl CoreTime {
    pub(crate) fn new(device: Arc<DeviceTime>, cache_lines: Option<usize>) -> Self {
        Self {
            device,
            maintenance: Calendar::default(),
            transport: Calendar::default(),
            cache: cache_lines.map(|lines| {
                assert!(lines >= 2 && lines.is_power_of_two());
                vec![None; lines]
            }),
        }
    }

    pub(crate) fn transfer(
        &mut self,
        start: u64,
        rows: u64,
        ranges: impl Iterator<Item = (u64, usize)> + Clone,
        local: Option<u64>,
        write: bool,
    ) -> u64 {
        let cpus = self.device.cpus.get().expect("CPU cache participants configured");
        let mut lines = Vec::new();
        let mut previous = None;
        let mut maintenance = 0;
        for (address, bytes) in ranges.clone() {
            for line in (address >> 6)..=((address + bytes as u64 - 1) >> 6) {
                if previous == Some(line) {
                    continue;
                }
                previous = Some(line);
                let state = cpus
                    .iter()
                    .fold(Residency::Absent, |state, cpu| state.max(cpu.maintain(line, write)));
                maintenance += match state {
                    Residency::Absent => 13,
                    Residency::Clean => 31,
                    Residency::Dirty => 41,
                };
                lines.push((line, state));
            }
        }
        // CleanInvalidate removes the destination from the DMA cache too.
        if write {
            if let Some(cache) = &mut self.cache {
                for entry in cache {
                    if entry.is_some_and(|(line, _)| {
                        ranges
                            .clone()
                            .any(|(address, bytes)| (line << 6) < address + bytes as u64 && address < (line + 1) << 6)
                    }) {
                        *entry = None;
                    }
                }
            }
        }
        let mut first = 0;
        let mut gaps = 0;
        let mut evictions = 0;
        if local.is_none() {
            for (index, &(line, state)) in lines.iter().enumerate() {
                let mut hit = false;
                if let Some(cache) = &mut self.cache {
                    let slot = line as usize & (cache.len() - 1);
                    let entry = &mut cache[slot];
                    if state == Residency::Dirty && entry.is_some_and(|(tag, _)| tag == line) {
                        *entry = None;
                    }
                    hit = entry.is_some_and(|(tag, _)| tag == line);
                    if let Some((tag, dirty)) = *entry {
                        if tag != line {
                            let owner = cpus
                                .iter()
                                .fold(Residency::Absent, |state, cpu| state.max(cpu.state(tag)));
                            if owner != Residency::Dirty {
                                evictions += if dirty && owner == Residency::Absent { 20 } else { 12 };
                            }
                        }
                    }
                    *entry = Some((line, write));
                }
                if !write {
                    if index == 0 {
                        if !hit && state == Residency::Absent {
                            first = 24;
                        }
                    } else if !hit && self.cache.is_some() {
                        gaps += if state == Residency::Absent { 27 } else { 16 };
                    }
                }
            }
        }
        let minimum = self.device.minimum();
        let setup = if local.is_some() {
            80
        } else if write {
            62
        } else {
            89
        };
        let prepared = self.maintenance.reserve(start, setup + maintenance, minimum);
        let end = if let Some(transport) = local {
            prepared + transport
        } else {
            let transport = if write {
                if self.cache.is_some() {
                    84 + 24 * rows + evictions
                } else {
                    20 + 4 * rows
                }
            } else {
                first + 5 * rows - 5 + gaps + evictions + 13
            };
            let core_end = self.transport.reserve(prepared, transport, minimum);
            // Backing bandwidth overlaps the core's cache/AXI service time.
            let ddr_end =
                self.device
                    .ddr
                    .lock()
                    .unwrap()
                    .reserve(core_end - transport, 4 * lines.len() as u64, minimum);
            core_end.max(ddr_end)
        };
        end - start
    }
}

#[cfg(test)]
mod tests {
    use super::{Calendar, CpuCache, Residency};

    #[test]
    fn cache_maintenance_distinguishes_dirty_clean_and_evicted_lines() {
        let cache = CpuCache::new(2);
        let address = 0x8000_0000;
        cache.access(address, true);
        cache.access(address, false);
        assert_eq!(cache.maintain(address >> 6, false), Residency::Dirty);
        assert_eq!(cache.maintain(address >> 6, false), Residency::Clean);
        cache.access(address + 128, false);
        assert_eq!(cache.state(address >> 6), Residency::Absent);
        cache.access(address + 64, true);
        assert_eq!(cache.maintain((address + 64) >> 6, true), Residency::Dirty);
        assert_eq!(cache.state((address + 64) >> 6), Residency::Absent);
    }

    #[test]
    fn a_late_host_thread_can_fill_a_gap_in_virtual_time() {
        let mut calendar = Calendar::default();
        assert_eq!(calendar.reserve(100, 20, 0), 120);
        assert_eq!(calendar.reserve(10, 20, 0), 30);
        assert_eq!(calendar.reserve(20, 10, 0), 40);
        assert_eq!(calendar.reserve(90, 20, 0), 140);
        assert_eq!(calendar.reserve(150, 10, 140), 160);
        assert_eq!(calendar.0.len(), 1);
    }
}
