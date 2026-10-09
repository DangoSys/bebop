use bebop_bank_hash::{cancel, failure, finish, progress, start, subject_counts, subject_matched};
use bebop_bemu::root::chip::Chip;
use bebop_bemu::{
    core_hart_id, core_is_ant, core_signature, hart_capacity, tile_count, tile_topology, Core, Tile, TraceConfig as BemuTraceConfig,
};
use snafu::{FromString, ResultExt, Whatever};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

struct GoldenHart {
    bemu: Core,
    hart_id: usize,
    waiting: bool,
}

pub struct DiffSession {
    golden: Vec<GoldenHart>,
    barriers: Vec<Vec<usize>>,
    next_hart: usize,
    active: bool,
}

impl DiffSession {
    pub fn new(elf: &Path, log_dir: &Path, memory_size: usize) -> Result<Self, Whatever> {
        let output = log_dir.join("diff.ndjson");
        start(output)?;

        let golden_result = (|| {
            let chip = Chip::new(memory_size, hart_capacity());
            let mut golden = Vec::with_capacity(hart_capacity());
            let mut barriers = Vec::new();
            for tile_index in 0..tile_count() {
                let topology = tile_topology(tile_index);
                let signatures = if topology.controller_core.is_some() {
                    topology.worker_cores.iter().map(|(_, core)| core_signature(*core)).collect()
                } else { Vec::new() };
                // Controller golden models own local Ant contexts; only real CPUs join CLINT.
                // NPU traces use PB core.index, independently of the CPU mhartid.
                let memory = Tile::new(&chip, &topology, signatures);
                let mut participants = Vec::new();
                for (_, core_index) in topology.cores.into_iter().filter(|(_,index)| !core_is_ant(*index)) {
                    if topology.endpoint_cores.iter().any(|(_, core)| *core == core_index) {
                        participants.push(golden.len());
                    }
                    let hart_id = core_hart_id(core_index);
                    let mut trace = BemuTraceConfig::new(false, false);
                    trace.btrace = true;
                    let mut bemu = Core::new_with_core_hart(
                        &log_dir.join("golden").join(format!("hart-{hart_id}")),
                        trace, false, false, core_index, hart_id, Some(memory.clone()),
                    ).whatever_context("failed to create BEMU Golden Model")?;
                    bemu.load_elf(elf)?;
                    bemu.init_hart()?;
                    golden.push(GoldenHart { bemu, hart_id, waiting: false });
                }
                if !participants.is_empty() { barriers.push(participants); }
            }
            chip.coordinate_harts(golden.iter().map(|hart| hart.hart_id).collect());
            Ok::<_, Whatever>((golden, barriers))
        })();
        let (golden, barriers) = match golden_result {
            Ok(models) => models,
            Err(error) => {
                cancel();
                return Err(error);
            }
        };

        Ok(Self {
            golden,
            barriers,
            next_hart: 0,
            active: true,
        })
    }

    pub fn sync_golden(&mut self, deadline: Option<Instant>) -> Result<(), Whatever> {
        let target = progress().subject;
        while !subject_matched() {
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Err(Whatever::without_source(format!(
                    "BTrace comparison drain timed out: {:?}",
                    progress()
                )));
            }
            if self.golden.iter().all(|hart| hart.bemu.finished()) {
                return Err(Whatever::without_source(format!(
                    "BEMU Golden Model finished before RTL hash boundary {target}"
                )));
            }

            for participants in &self.barriers {
                if participants.iter().all(|&index| self.golden[index].waiting) {
                    for &index in participants { self.golden[index].waiting = false; }
                }
            }

            let hart_index = (0..self.golden.len())
                .map(|offset| (self.next_hart + offset) % self.golden.len())
                .find(|&index| !self.golden[index].waiting && !self.golden[index].bemu.finished())
                .expect("at least one golden hart is runnable");
            let hart = &mut self.golden[hart_index];
            hart.bemu.step(1)?;
            hart.waiting = hart.bemu.take_barrier();
            if hart.bemu.finished() && hart.bemu.exit_code() != Some(0) {
                return Err(Whatever::without_source(format!(
                    "BEMU Golden Model hart {} exited with code {:?}",
                    hart.hart_id, hart.bemu.exit_code()
                )));
            }
            self.next_hart = (hart_index + 1) % self.golden.len();
            if let Some(message) = failure() {
                return Err(Whatever::without_source(message));
            }
        }
        if let Some(message) = failure() {
            return Err(Whatever::without_source(message));
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<(), Whatever> {
        self.active = false;
        finish()
    }

    pub fn drain_status(&mut self, expected: &BTreeMap<u64, u64>, deadline: Instant) -> Result<bool, Whatever> {
        let received = subject_counts();
        for (&hart, &count) in &received {
            let target = expected
                .get(&hart)
                .ok_or_else(|| Whatever::without_source(format!("BTrace received unexpected execution ID {hart}")))?;
            if count > *target {
                return Err(Whatever::without_source(format!(
                    "BTrace received too many events: execution={hart} expected={target} received={count}"
                )));
            }
        }
        self.sync_golden(Some(deadline))?;
        Ok(expected
            .iter()
            .all(|(hart, count)| received.get(hart).copied().unwrap_or(0) == *count)
            && subject_matched())
    }
}

impl Drop for DiffSession {
    fn drop(&mut self) {
        if self.active {
            cancel();
        }
    }
}
