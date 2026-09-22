use bebop_bank_hash::{cancel, failure, finish, progress, start};
use bebop_bemu::{BemuInstance, TraceConfig as BemuTraceConfig};
use snafu::{FromString, ResultExt, Whatever};
use std::path::Path;

pub struct DiffSession {
    golden: BemuInstance,
    active: bool,
}

impl DiffSession {
    pub fn new(elf: &Path, log_dir: &Path) -> Result<Self, Whatever> {
        let output = log_dir.join("diff.ndjson");
        start(output)?;

        let golden_result = (|| {
            let mut trace = BemuTraceConfig::new(false, false);
            trace.btrace = true;
            let mut golden = BemuInstance::new(&log_dir.join("golden"), trace, false, false)
                .whatever_context("failed to create BEMU Golden Model")?;
            golden.load_elf(elf)?;
            golden.init_hart(false)?;
            Ok::<_, Whatever>(golden)
        })();
        let golden = match golden_result {
            Ok(golden) => golden,
            Err(error) => {
                cancel();
                return Err(error);
            }
        };

        Ok(Self { golden, active: true })
    }

    pub fn sync_golden(&mut self) -> Result<(), Whatever> {
        let target = progress().subject;
        while progress().golden < target {
            if self.golden.finished() {
                return Err(Whatever::without_source(format!(
                    "BEMU Golden Model finished before RTL hash boundary {target}"
                )));
            }
            self.golden.step(1)?;
            self.check_comparison()?;
        }
        let status = progress();
        if status.golden > target {
            return Err(Whatever::without_source(format!(
                "BEMU Golden Model advanced past RTL hash boundary: golden={} rtl={target}",
                status.golden
            )));
        }
        self.check_comparison()
    }

    pub fn check_comparison(&self) -> Result<(), Whatever> {
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
