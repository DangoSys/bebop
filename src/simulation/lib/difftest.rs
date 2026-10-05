use bebop_bank_hash::{cancel, failure, finish, progress, start, subject_counts, subject_matched};
use bebop_bemu::root::chip::Chip;
use bebop_bemu::{
    core_hart_id, core_signature, hart_capacity, tile_count, tile_topology, Core, Tile, TraceConfig as BemuTraceConfig,
};
use snafu::{FromString, ResultExt, Whatever};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

struct GoldenHart {
    bemu: Core,
    waiting: bool,
}

pub struct DiffSession {
    golden: Vec<GoldenHart>,
    next_hart: usize,
    active: bool,
}

impl DiffSession {
    pub fn new(elf: &Path, log_dir: &Path) -> Result<Self, Whatever> {
        // This runner starts one ELF across one Tile. Do not silently execute a
        // multi-Tile RTL trace against Tile0-only golden harts.
        if tile_count() != 1 {
            return Err(Whatever::without_source(
                "bank diff runner currently supports exactly one Tile; multi-Tile golden orchestration is required"
                    .into(),
            ));
        }
        let output = log_dir.join("diff.ndjson");
        start(output)?;

        let golden_result = (|| {
            let topology = tile_topology(0);
            if !topology.has_buckyball {
                return Ok(Vec::new());
            }
            let signatures = if topology.controller_core.is_some() {
                topology
                    .worker_cores
                    .iter()
                    .map(|(_, core)| core_signature(*core))
                    .collect()
            } else {
                Vec::new()
            };
            let memory = Tile::new(&Chip::new(3 * (1 << 30), hart_capacity()), &topology, signatures);
            let mut golden = Vec::with_capacity(topology.cores.len());
            for (_, core_index) in topology.cores {
                let hart_id = core_hart_id(core_index);
                let mut trace = BemuTraceConfig::new(false, false);
                trace.btrace = true;
                let mut bemu = Core::new_with_core_hart(
                    &log_dir.join("golden").join(format!("hart-{hart_id}")),
                    trace,
                    false,
                    false,
                    core_index,
                    hart_id,
                    Some(memory.clone()),
                )
                .whatever_context("failed to create BEMU Golden Model")?;
                bemu.load_elf(elf)?;
                bemu.init_hart()?;
                golden.push(GoldenHart { bemu, waiting: false });
            }
            Ok::<_, Whatever>(golden)
        })();
        let golden = match golden_result {
            Ok(golden) => golden,
            Err(error) => {
                cancel();
                return Err(error);
            }
        };

        Ok(Self {
            golden,
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

            if self.golden.iter().all(|hart| hart.waiting || hart.bemu.finished()) {
                for hart in &mut self.golden {
                    hart.waiting = false;
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
                    "BEMU Golden Model hart {hart_index} exited with code {:?}",
                    hart.bemu.exit_code()
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
                .ok_or_else(|| Whatever::without_source(format!("BTrace received unexpected hart {hart}")))?;
            if count > *target {
                return Err(Whatever::without_source(format!(
                    "BTrace received too many events: hart={hart} expected={target} received={count}"
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
