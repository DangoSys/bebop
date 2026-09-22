use bebop_bank_hash::{cancel, failure, finish, progress, start, subject_matched};
use bebop_bemu::{tile_topology, BemuInstance, SharedMemory, TraceConfig as BemuTraceConfig};
use snafu::{FromString, ResultExt, Whatever};
use std::path::Path;

struct GoldenHart {
    bemu: BemuInstance,
    waiting: bool,
}

pub struct DiffSession {
    golden: Vec<GoldenHart>,
    next_hart: usize,
    active: bool,
}

impl DiffSession {
    pub fn new(elf: &Path, log_dir: &Path) -> Result<Self, Whatever> {
        let output = log_dir.join("diff.ndjson");
        start(output)?;

        let golden_result = (|| {
            let topology = tile_topology(0);
            if !topology.has_buckyball {
                return Ok(Vec::new());
            }
            let memory = SharedMemory::new(
                3 * (1 << 30),
                topology.cores.len(),
                topology.shared_physical_bank_count,
                topology.shared_bank_size,
                topology.virtual_bank_count,
            );
            let virtual_bank_count = (topology.shared_physical_bank_count != 0).then_some(topology.virtual_bank_count);
            let mut golden = Vec::with_capacity(topology.cores.len());
            for (hart_id, (_, core_index)) in topology.cores.into_iter().enumerate() {
                let mut trace = BemuTraceConfig::new(false, false);
                trace.btrace = true;
                let mut bemu = BemuInstance::new_with_core_hart(
                    &log_dir.join("golden").join(format!("hart-{hart_id}")),
                    trace,
                    false,
                    false,
                    core_index,
                    hart_id,
                    Some(memory.clone()),
                    virtual_bank_count,
                )
                .whatever_context("failed to create BEMU Golden Model")?;
                bemu.load_elf(elf)?;
                bemu.init_hart(false)?;
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

    pub fn sync_golden(&mut self) -> Result<(), Whatever> {
        let target = progress().subject;
        while !subject_matched() {
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
}

impl Drop for DiffSession {
    fn drop(&mut self) {
        if self.active {
            cancel();
        }
    }
}
