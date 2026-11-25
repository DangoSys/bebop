use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
};

/// Bank DMA wiring: source zero is the hart, followed by its local threads.
pub struct DmaLayout {
    pub read_row_cycles: u64,
    /// A Home-local range scanner skips a line absent from every CPU in one cycle.
    pub range_maintenance: bool,
    pub channel_by_source: Vec<usize>,
}

/// The device payload endpoint's launch, streamed SRAM work, and final acknowledgement.
pub struct ServicePhases {
    pub prepare: u64,
    pub stream: u64,
    pub retire: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Residency {
    Absent,
    Clean,
    Dirty,
}

pub struct CpuCache {
    tags: Vec<AtomicU64>,
    highest_line: AtomicU64,
}

impl CpuCache {
    pub fn new(lines: usize) -> Self {
        assert!(lines >= 2 && lines.is_power_of_two());
        Self {
            tags: (0..lines).map(|_| AtomicU64::new(u64::MAX)).collect(),
            highest_line: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn access(&self, address: u64, write: bool) {
        let line = address >> 6;
        let slot = &self.tags[line as usize & (self.tags.len() - 1)];
        let old = slot.load(Ordering::Relaxed);
        let dirty = write || (old >> 1 == line && old & 1 != 0);
        let new = (line << 1) | u64::from(dirty);
        if write || new != old {
            // Publish the bound before a new tag. Eviction never lowers it.
            if old >> 1 != line && line > self.highest_line.load(Ordering::Relaxed) {
                self.highest_line.fetch_max(line, Ordering::Release);
            }
            slot.store(new, Ordering::Release);
        }
    }

    fn state(&self, line: u64) -> Residency {
        if line > self.highest_line.load(Ordering::Acquire) {
            return Residency::Absent;
        }
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
        if line > self.highest_line.load(Ordering::Acquire) {
            return Residency::Absent;
        }
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
pub struct Calendar {
    intervals: BTreeMap<u64, u64>,
    busy_cycles: u64,
}

impl Calendar {
    pub fn reserve(&mut self, mut start: u64, duration: u64, minimum: u64) -> u64 {
        while self.intervals.first_key_value().is_some_and(|(_, &end)| end <= minimum) {
            self.intervals.pop_first();
        }
        if let Some((_, &end)) = self.intervals.range(..=start).next_back() {
            start = start.max(end);
        }
        for (&next, &end) in self.intervals.range(start..) {
            if start + duration <= next {
                break;
            }
            start = end;
        }
        self.intervals.insert(start, start + duration);
        // Reservations do not overlap; retired intervals still count as bus service.
        self.busy_cycles += duration;
        start + duration
    }
}

pub struct DeviceTime {
    pub threads: OnceLock<Vec<Arc<AtomicU64>>>,
    pub cpus: OnceLock<Vec<Arc<CpuCache>>>,
    clint: Arc<crate::clint::Clint>,
    ddr: Mutex<Calendar>,
    /// The bank rows of a beat of the DDR port: a transfer's rows take that many fewer beats.
    ddr_beat_rows: u64,
}

impl DeviceTime {
    pub fn new(clint: Arc<crate::clint::Clint>, ddr_beat_rows: u64) -> Self {
        Self {
            threads: OnceLock::new(),
            cpus: OnceLock::new(),
            clint,
            ddr: Mutex::new(Calendar::default()),
            ddr_beat_rows,
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

    pub fn profile_report(&self) -> String {
        let total = self.clint.cycles();
        let ddr = self.ddr.lock().unwrap();
        let reserved = ddr.busy_cycles;
        let pending: u64 = ddr
            .intervals
            .iter()
            .map(|(&start, &end)| {
                if start >= total {
                    end - start
                } else if end > total {
                    end - total
                } else {
                    0
                }
            })
            .sum();
        let busy = reserved - pending;
        let idle = total
            .checked_sub(busy)
            .expect("DDR service exceeds elapsed device cycles");
        format!(
            "BEMU modeled accelerator DDR service (whole boot/run):\n\
             total_cycles={total} reserved_cycles={reserved} busy_cycles={busy} idle_cycles={idle} pending_cycles={pending} ddr_beat_rows={}\n\
             Scope: accelerator/service DDR reservations only; CPU miss/writeback bandwidth is not counted.\n\
             This is modeled service occupancy, not physical DDR handshake utilization.",
            self.ddr_beat_rows
        )
    }
}

pub struct HartTime {
    pub device: Arc<DeviceTime>,
    maintenance: Calendar,
    transport: Vec<Calendar>,
    dma: DmaLayout,
}

impl HartTime {
    pub fn stage(
        &mut self,
        source: usize,
        start: u64,
        ranges: impl Iterator<Item = (u64, usize)>,
        phases: &ServicePhases,
    ) -> u64 {
        let ranges: Vec<_> = ranges.collect();
        let total_rows: u64 = ranges.iter().map(|(_, bytes)| (*bytes as u64).div_ceil(16)).sum();
        let cpus = self.device.cpus.get().expect("CPU cache participants configured");
        let minimum = self.device.minimum();
        let mut end = start + phases.prepare;
        let mut first = true;
        let mut consumed_rows = 0;
        for (address, bytes) in ranges {
            assert!(address & 15 == 0 && bytes > 0 && (address & 4095) + bytes as u64 <= 4096);
            let page = (address >> 12) << 6;
            let resident = (page..page + 64).any(|line| cpus.iter().any(|cpu| cpu.state(line) != Residency::Absent));
            let mut maintenance = 0;
            if resident {
                for line in (address >> 6)..=((address + bytes as u64 - 1) >> 6) {
                    let state = cpus
                        .iter()
                        .fold(Residency::Absent, |state, cpu| state.max(cpu.maintain(line, false)));
                    maintenance += match state {
                        Residency::Absent => {
                            if self.dma.range_maintenance {
                                1
                            } else {
                                2
                            }
                        }
                        Residency::Clean => 2,
                        Residency::Dirty => 41,
                    };
                }
            } else {
                maintenance = 1;
            }
            if !first {
                // ReadTransport consumes the terminal result, then plans the next page burst.
                end += 2;
            }
            first = false;
            // DmaMemory: select/translate/walker reply/authorize/permission reply/maintain.
            // The qualified M/bare, unlocked-PMP-OFF path takes six cycles; <=64B uses
            // the one-cycle-longer PermissionCheck path. Other privilege/PMP timing is
            // not qualified by these service microbenchmarks.
            let prepared = self
                .maintenance
                .reserve(end, 6 + u64::from(bytes <= 64) + maintenance, minimum);
            let rows = (bytes as u64).div_ceil(16);
            let previous = phases.stream * consumed_rows / total_rows;
            consumed_rows += rows;
            let local = phases.stream * consumed_rows / total_rows - previous;
            // DmaMemory AR issue, ordinary DDR return, TLink credit/receive and first R.
            // ReadTransport's elastic result register sustains one row per cycle. Payload
            // SRAM requests overlap these rows only after this burst's maintenance.
            let stream = 14 + (rows - 1).max(local.saturating_sub(1));
            let memory_end = self.transport[self.dma.channel_by_source[source]].reserve(prepared, stream, minimum);
            let ddr_end = self.device.ddr.lock().unwrap().reserve(
                memory_end - stream,
                (rows.div_ceil(4) * 4).div_ceil(self.device.ddr_beat_rows),
                minimum,
            );
            end = memory_end.max(ddr_end);
        }
        end + phases.retire - start
    }

    pub fn new(device: Arc<DeviceTime>, dma: DmaLayout) -> Self {
        let channels = dma.channel_by_source.iter().max().expect("DMA has no sources") + 1;
        Self {
            device,
            maintenance: Calendar::default(),
            transport: (0..2 * channels).map(|_| Calendar::default()).collect(),
            dma,
        }
    }

    pub fn transfer(
        &mut self,
        source: usize,
        start: u64,
        rows: u64,
        ranges: impl Iterator<Item = (u64, usize)>,
        local: Option<u64>,
        write: bool,
    ) -> u64 {
        let ranges: Vec<_> = ranges.collect();
        // Local storage has no DDR footprint, but still supplies its latency.
        if ranges.is_empty() {
            return match local {
                Some(cycles) => cycles,
                None => 0,
            };
        }
        let cpus = self.device.cpus.get().expect("CPU cache participants configured");
        let mut bursts = 0;
        let mut beats = 0;
        let mut drain_cycles = 0;
        let mut single_rows = 0;
        let row_bytes = ranges
            .iter()
            .map(|(_, bytes)| *bytes as u64)
            .sum::<u64>()
            .div_ceil(rows);
        let mut spans: Vec<(u64, u64)> = Vec::new();
        let mut maintenance = 0;
        let mut first = None;
        // Preparation unites the rows of one page, gaps included, into one maintained range.
        let mut union: Option<(u64, u64)> = None;
        let mut maintain = |low: u64, high: u64| {
            // The Home answers whether the page has a line in any cache; a page with none skips its lines.
            let page = (low >> 12) << 6;
            if (page..page + 64).all(|line| cpus.iter().all(|cpu| cpu.state(line) == Residency::Absent)) {
                first.get_or_insert(Residency::Absent);
                maintenance += 1;
                return;
            }
            for line in (low >> 6)..=(high >> 6) {
                let state = cpus
                    .iter()
                    .fold(Residency::Absent, |state, cpu| state.max(cpu.maintain(line, write)));
                first.get_or_insert(state);
                // A line the coherence home does not need costs only the scan.
                maintenance += match state {
                    Residency::Absent => {
                        if self.dma.range_maintenance {
                            1
                        } else {
                            2
                        }
                    }
                    Residency::Clean if !write => 2,
                    Residency::Clean => 31,
                    Residency::Dirty => 41,
                };
            }
        };
        for (address, bytes) in ranges {
            let end = address + bytes as u64;
            if let Some((base, size)) = spans.last_mut().filter(|(base, size)| *base + *size == address) {
                *size = end - *base;
            } else {
                spans.push((address, bytes as u64));
            }
            let mut at = address;
            while at < end {
                let stop = end.min(((at >> 12) + 1) << 12);
                union = match union {
                    Some((low, _)) if low >> 12 == at >> 12 => Some((low, stop - 1)),
                    Some((low, high)) => {
                        maintain(low, high);
                        Some((at, stop - 1))
                    }
                    None => Some((at, stop - 1)),
                };
                at = stop;
            }
        }
        // A bank row is the transport's unit. Unaligned reads reissue one
        // single-beat AR for each half of that row; adjacent checked ranges
        // do not turn them into an aligned burst. Partial rows never burst.
        let divide_shape = spans.len() > 1 && row_bytes == 16 && spans.iter().any(|(_, bytes)| *bytes > 16);
        for (address, bytes) in spans {
            let end = address + bytes;
            let mut at = address;
            if row_bytes < 16 || (!write && address & 15 != 0) {
                while at < end {
                    let count = row_bytes.min(end - at);
                    let count_beats = ((at & 15) + count).div_ceil(16);
                    bursts += if write {
                        1 + u64::from((at & 4095) + count > 4096)
                    } else {
                        count_beats
                    };
                    drain_cycles += if write && (at & 4095) + count > 4096 {
                        2 * 4u64.div_ceil(self.device.ddr_beat_rows)
                    } else {
                        (count_beats.div_ceil(4) * 4).div_ceil(self.device.ddr_beat_rows)
                    };
                    single_rows += 1;
                    beats += count_beats;
                    at += count;
                }
            } else {
                while at < end {
                    let count = (4096 - (at & 4095)).min(end - at);
                    bursts += 1;
                    let count_beats = ((at & 15) + count).div_ceil(16);
                    beats += count_beats;
                    drain_cycles += (count_beats.div_ceil(4) * 4).div_ceil(self.device.ddr_beat_rows);
                    at += count;
                }
            }
        }
        // A DMA of no bytes moves nothing and maintains no line.
        let Some((low, high)) = union else {
            return 0;
        };
        maintain(low, high);
        let first = first.expect("DMA maintains a line");
        let minimum = self.device.minimum();
        let setup = if local.is_some() {
            80
        } else if write {
            20
        } else {
            24
        };
        // Each producer prepares its descriptor independently. The shared
        // coherence engine serializes maintenance, not the frontend setup.
        let prepared = self.maintenance.reserve(start + setup, maintenance, minimum);
        let end = if let Some(transport) = local {
            prepared + transport
        } else {
            let transport = if write {
                // The bridge collects a complete narrow burst, drains it at
                // DDR width, then returns B in six cycles. The next AW takes
                // eleven cycles. Coherence ran once before the write.
                rows + drain_cycles + 6 * bursts + 11 * (bursts - 1)
            } else {
                if single_rows != 0 {
                    // Captured AR handshakes: 18 cycles between single-beat
                    // transactions, plus three cycles to advance a bank row.
                    12 + 18 * (bursts - 1) + 3 * (single_rows - 1) + 16 * u64::from(divide_shape)
                } else {
                    let cold = if first == Residency::Absent { 4 } else { 0 };
                    cold + rows * self.dma.read_row_cycles
                        + 8
                        + 26 * (bursts - 1).min(100)
                        + 48 * (bursts - 1).saturating_sub(100)
                        + 16 * u64::from(divide_shape)
                }
            };
            // TLink has independent read and write streams. Their common DDR
            // bus is still accounted for by the device bandwidth calendar.
            let channel = self.dma.channel_by_source[source] + if write { self.transport.len() / 2 } else { 0 };
            let hart_end = self.transport[channel].reserve(prepared, transport, minimum);
            // Reads stream from DDR. Writes cannot drain the collected burst
            // before its last narrow W beat; include that service in the hart
            // duration instead of assuming it overlaps the input stream.
            let ddr_end = self.device.ddr.lock().unwrap().reserve(
                hart_end - if write { drain_cycles } else { transport },
                if write {
                    drain_cycles
                } else {
                    (beats.div_ceil(4) * 4).div_ceil(self.device.ddr_beat_rows)
                },
                minimum,
            );
            hart_end.max(ddr_end)
        };
        end - start
    }
}

#[cfg(test)]
mod tests {
    use super::{Calendar, CpuCache, DeviceTime, DmaLayout, HartTime, Residency, ServicePhases};
    use std::sync::Arc;

    #[test]
    fn service_stage_prepares_each_burst_after_the_previous_stream() {
        let device = Arc::new(DeviceTime::new(Arc::new(crate::clint::Clint::new(1)), 4));
        assert!(device.threads.set(Vec::new()).is_ok());
        let cache = Arc::new(CpuCache::new(128));
        assert!(device.cpus.set(vec![cache.clone()]).is_ok());
        cache.access(0x8000_0fc0, true);
        cache.access(0x8000_17b0, true);
        let mut hart = HartTime::new(
            device,
            DmaLayout {
                read_row_cycles: 10,
                range_maintenance: true,
                channel_by_source: vec![0],
            },
        );
        let duration = hart.stage(
            0,
            100,
            [(0x8000_0fc0, 64), (0x8000_1000, 1984)].into_iter(),
            &ServicePhases {
                prepare: 7,
                stream: 128,
                retire: 7,
            },
        );
        let preparations: Vec<_> = hart.maintenance.intervals.iter().map(|(&a, &b)| (a, b)).collect();
        let streams: Vec<_> = hart.transport[0].intervals.iter().map(|(&a, &b)| (a, b)).collect();
        assert_eq!(preparations.len(), 2);
        assert_eq!(streams.len(), 2);
        assert!(preparations[1].0 >= streams[0].1 + 2);
        assert_eq!(duration, streams[1].1 + 7 - 100);
        assert_eq!(cache.state(0x8000_0fc0 >> 6), Residency::Clean);
        assert_eq!(cache.state(0x8000_17b0 >> 6), Residency::Clean);
        assert!(!hart.device.ddr.lock().unwrap().intervals.is_empty());
    }

    #[test]
    fn local_transfers_keep_their_latency_without_touching_ddr_or_cpu_cache() {
        let device = Arc::new(DeviceTime::new(Arc::new(crate::clint::Clint::new(1)), 1));
        assert!(device.threads.set(Vec::new()).is_ok());
        let cache = Arc::new(CpuCache::new(4));
        assert!(device.cpus.set(vec![cache.clone()]).is_ok());
        let address = 0x8000_0000;
        cache.access(address, true);
        let mut hart = HartTime::new(
            device.clone(),
            DmaLayout {
                read_row_cycles: 1,
                range_maintenance: false,
                channel_by_source: vec![0],
            },
        );
        assert_eq!(hart.transfer(0, 100, 4, std::iter::empty(), Some(26), false), 26);
        assert_eq!(hart.transfer(0, 200, 4, std::iter::empty(), Some(30), true), 30);
        assert_eq!(cache.state(address >> 6), Residency::Dirty);
        assert!(hart.maintenance.intervals.is_empty());
        assert!(hart.transport.iter().all(|channel| channel.intervals.is_empty()));
        assert!(device.ddr.lock().unwrap().intervals.is_empty());
    }

    #[test]
    fn home_range_scans_empty_lines_locally_without_changing_dirty_maintenance() {
        for bytes in [2048, 4096, 8192] {
            for write in [false, true] {
                let mut results = Vec::new();
                for range_maintenance in [false, true] {
                    let device = Arc::new(DeviceTime::new(Arc::new(crate::clint::Clint::new(1)), 4));
                    assert!(device.threads.set(Vec::new()).is_ok());
                    let cache = Arc::new(CpuCache::new(128));
                    assert!(device.cpus.set(vec![cache.clone()]).is_ok());
                    let address = 0x8000_0000;
                    cache.access(address, true);
                    let mut hart = HartTime::new(
                        device,
                        DmaLayout {
                            read_row_cycles: 2,
                            range_maintenance,
                            channel_by_source: vec![0],
                        },
                    );
                    let cycles = hart.transfer(0, 0, bytes / 16, [(address, bytes as usize)].into_iter(), None, write);
                    assert_eq!(
                        cache.state(address >> 6),
                        if write { Residency::Absent } else { Residency::Clean }
                    );
                    results.push(cycles);
                }
                // Only the first page is resident. Its remaining lines use Home-local probes;
                // the following fully absent page still takes the existing page-query path.
                assert_eq!(results[0] - results[1], (bytes.min(4096) / 64) - 1);
            }
        }
    }

    #[test]
    fn repeated_dma_reads_do_not_retain_data() {
        let device = Arc::new(DeviceTime::new(Arc::new(crate::clint::Clint::new(1)), 1));
        assert!(device.threads.set(Vec::new()).is_ok());
        assert!(device.cpus.set(vec![Arc::new(CpuCache::new(4))]).is_ok());
        let mut hart = HartTime::new(
            device,
            DmaLayout {
                read_row_cycles: 1,
                range_maintenance: false,
                channel_by_source: vec![0],
            },
        );
        let first = hart.transfer(0, 0, 4, [(0x8000_0000, 64)].into_iter(), None, false);
        let second = hart.transfer(0, 1000, 4, [(0x8000_0000, 64)].into_iter(), None, false);
        assert_eq!(first, second);
    }

    #[test]
    fn calibrated_bank_reads_follow_the_rtl_transfer_range() {
        // DDR-to-bank transfers from columns-sliced-rtl.elf, including page boundaries, each alone.
        for (address, rows, rtl) in [
            (0x8000_4ec0, 1056, 13385),
            (0x8002_1cc0, 1848, 19227),
            (0x8000_4740, 33, 369),
        ] {
            let device = Arc::new(DeviceTime::new(Arc::new(crate::clint::Clint::new(1)), 1));
            assert!(device.threads.set(Vec::new()).is_ok());
            assert!(device.cpus.set(vec![Arc::new(CpuCache::new(4))]).is_ok());
            let mut hart = HartTime::new(
                device.clone(),
                DmaLayout {
                    read_row_cycles: 10,
                    range_maintenance: false,
                    channel_by_source: vec![0],
                },
            );
            let cycles = hart.transfer(0, 0, rows, [(address, rows as usize * 16)].into_iter(), None, false);
            assert!(cycles.abs_diff(rtl) * 5 <= rtl, "modeled {cycles}, RTL {rtl}");
        }
    }

    #[test]
    fn independent_bank_streams_share_only_backing_bandwidth() {
        let mut ends = Vec::new();
        for channels in [vec![0, 0], vec![0, 1]] {
            let device = Arc::new(DeviceTime::new(Arc::new(crate::clint::Clint::new(1)), 1));
            assert!(device.threads.set(Vec::new()).is_ok());
            assert!(device.cpus.set(vec![Arc::new(CpuCache::new(4))]).is_ok());
            let mut hart = HartTime::new(
                device,
                DmaLayout {
                    read_row_cycles: 10,
                    range_maintenance: false,
                    channel_by_source: channels,
                },
            );
            let first = hart.transfer(0, 0, 512, [(0x8000_0000, 8192)].into_iter(), None, false);
            let second = hart.transfer(1, 0, 512, [(0x8000_2000, 8192)].into_iter(), None, false);
            ends.push((first, second));
        }
        assert_eq!(ends[0].0, ends[1].0);
        assert!(ends[1].1 < ends[0].1);
        assert!(ends[1].1 > ends[1].0);
    }

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
        let high = 1u64 << 34;
        assert_eq!(cache.maintain(high >> 6, true), Residency::Absent);
        cache.access(high, true);
        assert_eq!(cache.maintain(high >> 6, false), Residency::Dirty);
        cache.access(address, false);
        assert_eq!(cache.state(high >> 6), Residency::Absent);
        assert_eq!(cache.state(address >> 6), Residency::Clean);
        cache.access(high + 64, true);
        assert_eq!(cache.maintain((high + 64) >> 6, true), Residency::Dirty);
    }

    #[test]
    fn a_late_host_thread_can_fill_a_gap_in_virtual_time() {
        let mut calendar = Calendar::default();
        assert_eq!(calendar.reserve(100, 20, 0), 120);
        assert_eq!(calendar.reserve(10, 20, 0), 30);
        assert_eq!(calendar.reserve(20, 10, 0), 40);
        assert_eq!(calendar.reserve(90, 20, 0), 140);
        assert_eq!(calendar.reserve(150, 10, 140), 160);
        assert_eq!(calendar.intervals.len(), 1);
        assert_eq!(calendar.busy_cycles, 80);
    }

    #[test]
    fn ddr_profile_counts_read_and_write_beats_once_after_retirement() {
        let clint = Arc::new(crate::clint::Clint::new(1));
        let device = Arc::new(DeviceTime::new(clint.clone(), 4));
        assert!(device.threads.set(Vec::new()).is_ok());
        assert!(device.cpus.set(vec![Arc::new(CpuCache::new(64))]).is_ok());
        let mut read = HartTime::new(
            device.clone(),
            DmaLayout {
                read_row_cycles: 1,
                range_maintenance: true,
                channel_by_source: vec![0],
            },
        );
        let mut write = HartTime::new(
            device.clone(),
            DmaLayout {
                read_row_cycles: 1,
                range_maintenance: true,
                channel_by_source: vec![0],
            },
        );
        let read_end = 100 + read.transfer(0, 100, 16, [(0x8000_0000, 256)].into_iter(), None, false);
        let write_end = 100 + write.transfer(0, 100, 16, [(0x8000_1000, 256)].into_iter(), None, true);
        {
            let mut ddr = device.ddr.lock().unwrap();
            assert_eq!(ddr.busy_cycles, 8);
            let intervals: Vec<_> = ddr.intervals.iter().map(|(&a, &b)| (a, b)).collect();
            assert_eq!(intervals.len(), 2);
            assert!(intervals[0].1 <= intervals[1].0);
            let drained = read_end.max(write_end);
            // A later reservation retires both intervals but preserves all eight beats.
            assert_eq!(ddr.reserve(drained + 100, 2, drained), drained + 102);
            assert_eq!(ddr.intervals.len(), 1);
            assert_eq!(ddr.busy_cycles, 10);
        }
        clint.advance_to(0, 10000);
        let report = device.profile_report();
        assert!(report.contains(
            "total_cycles=10000 reserved_cycles=10 busy_cycles=10 idle_cycles=9990 pending_cycles=0 ddr_beat_rows=4"
        ));
    }

    #[test]
    fn ddr_profile_reports_future_reservations_without_counting_them_as_elapsed() {
        let clint = Arc::new(crate::clint::Clint::new(1));
        let device = DeviceTime::new(clint.clone(), 4);
        assert_eq!(device.ddr.lock().unwrap().reserve(90, 20, 0), 110);
        assert_eq!(device.ddr.lock().unwrap().reserve(120, 5, 0), 125);
        clint.advance_to(0, 100);
        assert!(device
            .profile_report()
            .contains("total_cycles=100 reserved_cycles=25 busy_cycles=10 idle_cycles=90 pending_cycles=15"));
    }
}

/// A CPU's timing: the cycles from the previous instruction's retirement to this one's. `x` holds
/// the integer registers before the instruction.
pub trait CpuTiming: Send {
    /// The cycles of the instruction at `pc`: `decoded` is its 4-byte form, `operand` the register its rs1 field
    /// names, before it.
    fn instruction(&mut self, pc: u64, instruction: u32, decoded: u32, operand: u64) -> u64;
}

/// A cache's timing: the cycles an access to `address` waits beyond a hit.
pub trait CacheTiming: Send {
    fn access(&mut self, address: u64) -> u64;
    /// Drops every line, as fence.i or sfence.vma drops an I$.
    fn invalidate(&mut self);
}
