use bebop_bank_hash::{cancel, failure, finish, progress, start, subject_counts, subject_matched};
use bebop_bemu::{hart_capacity, CoreModel};
use bebop_bemu::{sim::run::Args, DeviceModel};
use clap::Parser;
use snafu::{FromString, ResultExt, Whatever};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

struct GoldenHart {
    bemu: Box<dyn CoreModel>,
    hart_id: usize,
    waiting: bool,
}

pub struct DiffSession {
    golden: Vec<GoldenHart>,
    barriers: Vec<Vec<usize>>,
    next_hart: usize,
    active: bool,
    _device: DeviceModel,
}

impl DiffSession {
    pub fn new(elf: &Path, log_dir: &Path, memory_size: usize) -> Result<Self, Whatever> {
        let output = log_dir.join("diff.ndjson");
        start(output)?;

        let golden_result = (|| {
            let simulation = crate::SIMULATION.get().expect("difftest requires a simulation entry");
            bebop_bemu::configs::load(simulation.design);
            let design = DeviceModel::new(memory_size, hart_capacity());
            let args = Args::try_parse_from([
                "golden",
                "--elf",
                elf.to_str().unwrap(),
                "--log-dir",
                log_dir.to_str().unwrap(),
                "--banktrace",
            ])
            .map_err(|error| Whatever::without_source(error.to_string()))?;
            let mut golden = Vec::new();
            let mut barriers = Vec::new();
            for mut bemu in (simulation.instantiate)(&design, &args).map_err(Whatever::without_source)? {
                let hart_id = bemu.cpu().hart_id();
                bemu.cpu_mut().load_elf(elf)?;
                bemu.cpu_mut().init_hart()?;
                barriers.push(vec![golden.len()]);
                golden.push(GoldenHart {
                    bemu,
                    hart_id,
                    waiting: false,
                });
            }
            design.coordinate_harts(golden.iter().map(|hart| hart.hart_id).collect());
            Ok::<_, Whatever>((golden, barriers, design))
        })();
        let (golden, barriers, device) = match golden_result {
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
            _device: device,
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
            if self.golden.iter().all(|hart| hart.bemu.cpu().finished()) {
                return Err(Whatever::without_source(format!(
                    "BEMU Golden Model finished before RTL hash boundary {target}"
                )));
            }

            for participants in &self.barriers {
                if participants.iter().all(|&index| self.golden[index].waiting) {
                    for &index in participants {
                        self.golden[index].waiting = false;
                    }
                }
            }

            let hart_index = (0..self.golden.len())
                .map(|offset| (self.next_hart + offset) % self.golden.len())
                .find(|&index| !self.golden[index].waiting && !self.golden[index].bemu.cpu().finished())
                .expect("at least one golden hart is runnable");
            let hart = &mut self.golden[hart_index];
            hart.bemu.step(1)?;
            hart.waiting = hart.bemu.take_barrier();
            if hart.bemu.cpu().finished() && hart.bemu.cpu().exit_code() != Some(0) {
                return Err(Whatever::without_source(format!(
                    "BEMU Golden Model hart {} exited with code {:?}",
                    hart.hart_id,
                    hart.bemu.cpu().exit_code()
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
