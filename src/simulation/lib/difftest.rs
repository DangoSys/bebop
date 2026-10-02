use bebop_bank_hash::{cancel, failure, finish, progress, start, subject_counts, subject_matched};
use bebop_bemu::root::chip::Chip;
use bebop_bemu::{tile_topology, Core, Tile, TraceConfig as BemuTraceConfig};
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
    access: bool,
    pub bemu_time_s: f64,
}

impl DiffSession {
    pub fn new(elf: &Path, log_dir: &Path) -> Result<Self, Whatever> {
        Self::with_access(elf, log_dir, false)
    }

    pub fn with_access(elf: &Path, log_dir: &Path, access: bool) -> Result<Self, Whatever> {
        let output = log_dir.join("diff.ndjson");
        if access {
            bebop_bank_hash::access::start();
        } else {
            start(output)?;
        }

        let golden_result = (|| {
            let topology = tile_topology(0);
            if !topology.has_buckyball {
                return Ok(Vec::new());
            }
            let memory = Tile::new(
                &Chip::new(3 * (1 << 30), topology.cores.len()),
                0,
                topology.cores.len(),
                Vec::new(),
                topology.shared_physical_bank_count,
                topology.shared_bank_size,
                topology.virtual_bank_count,
            );
            let mut golden = Vec::with_capacity(topology.cores.len());
            for (hart_id, (_, core_index)) in topology.cores.into_iter().enumerate() {
                let mut trace = BemuTraceConfig::new(false, false);
                trace.btrace = !access;
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
                bemu.load_elf(elf, false)?;
                bemu.init_hart(false)?;
                golden.push(GoldenHart { bemu, waiting: false });
            }
            Ok::<_, Whatever>(golden)
        })();
        let golden = match golden_result {
            Ok(golden) => golden,
            Err(error) => {
                if access {
                    bebop_bank_hash::access::cancel();
                } else {
                    cancel();
                }
                return Err(error);
            }
        };

        Ok(Self {
            golden,
            next_hart: 0,
            active: true,
            access,
            bemu_time_s: 0.0,
        })
    }

    pub fn sync_golden(&mut self, deadline: Option<Instant>) -> Result<(), Whatever> {
        let started = Instant::now();
        let target = if self.access {
            bebop_bank_hash::access::inspect(|s| s.subject_count)
        } else {
            progress().subject
        };
        while if self.access {
            !bebop_bank_hash::access::subject_matched()
        } else {
            !subject_matched()
        } {
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Err(Whatever::without_source(format!(
                    "comparison drain timed out at {target}"
                )));
            }
            if self.golden.iter().all(|hart| hart.bemu.finished()) {
                return Err(Whatever::without_source(format!(
                    "BEMU Golden Model finished before RTL hash boundary {target}"
                )));
            }

            self.step_golden()?;
            if let Some(message) = if self.access {
                bebop_bank_hash::access::failure()
            } else {
                failure()
            } {
                return Err(Whatever::without_source(message));
            }
        }
        if let Some(message) = if self.access {
            bebop_bank_hash::access::failure()
        } else {
            failure()
        } {
            return Err(Whatever::without_source(message));
        }
        self.bemu_time_s += started.elapsed().as_secs_f64();
        Ok(())
    }

    fn step_golden(&mut self) -> Result<(), Whatever> {
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
        Ok(())
    }

    pub fn complete_access_reference(&mut self, deadline: Instant) -> Result<(), Whatever> {
        assert!(self.access);
        let started = Instant::now();
        while self.golden.iter().any(|hart| !hart.bemu.finished()) {
            if Instant::now() >= deadline {
                return Err(Whatever::without_source("reference completion timed out".into()));
            }
            self.step_golden()?;
            if let Some(error) = bebop_bank_hash::access::failure() {
                return Err(Whatever::without_source(error));
            }
        }
        self.bemu_time_s += started.elapsed().as_secs_f64();
        Ok(())
    }

    pub fn finish(mut self) -> Result<(), Whatever> {
        self.active = false;
        if self.access {
            bebop_bank_hash::access::finish().map_err(Whatever::without_source)
        } else {
            finish()
        }
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
            if self.access {
                bebop_bank_hash::access::cancel();
            } else {
                cancel();
            }
        }
    }
}
