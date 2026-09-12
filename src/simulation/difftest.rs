use bebop_bank_hash::{
    init_runtime_packet_channel, run_online_compare_with_summary, runtime_packet_status,
    shutdown_runtime_packet_channel, BankHashCompareSummary,
};
use bebop_bemu::{BemuInstance, TraceConfig as BemuTraceConfig};
use snafu::{FromString, ResultExt, Whatever};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

pub struct DiffSession {
    golden: Option<BemuInstance>,
    golden_worker: Option<JoinHandle<Result<(), String>>>,
    compare_worker: Option<JoinHandle<Result<BankHashCompareSummary, String>>>,
    cancel_golden: Arc<AtomicBool>,
}

impl DiffSession {
    pub fn new(elf: &Path, log_dir: &Path, pk: bool) -> Result<Self, Whatever> {
        let receiver = init_runtime_packet_channel();
        let output = log_dir.join("bank_diff.ndjson");
        let compare_worker = std::thread::Builder::new()
            .name("bank-diff-m4".to_string())
            .spawn(move || run_online_compare_with_summary(receiver, output).map_err(|error| error.to_string()))
            .map_err(|error| {
                shutdown_runtime_packet_channel();
                Whatever::without_source(format!("failed to start Bank DiffTest M4 worker: {error}"))
            })?;

        let golden_result = (|| {
            let golden_log_dir = log_dir.join("golden");
            let mut trace = BemuTraceConfig::new(false, false);
            trace.btrace = true;
            let mut golden = BemuInstance::new(&golden_log_dir, trace, false, false)
                .whatever_context("failed to create BEMU Golden Model")?;
            golden.load_elf(elf)?;
            golden.init_hart(pk)?;
            Ok::<_, Whatever>(golden)
        })();
        let golden = match golden_result {
            Ok(golden) => golden,
            Err(error) => {
                shutdown_runtime_packet_channel();
                let _ = compare_worker.join();
                return Err(error);
            }
        };

        Ok(Self {
            golden: Some(golden),
            golden_worker: None,
            compare_worker: Some(compare_worker),
            cancel_golden: Arc::new(AtomicBool::new(false)),
        })
    }

    #[cfg(feature = "verilator")]
    pub fn step_golden(&mut self) -> Result<(), Whatever> {
        let golden = self.golden.as_mut().expect("foreground golden exists");
        if !golden.finished() {
            golden.step()?;
        }
        Ok(())
    }

    pub fn start_golden_background(&mut self) -> Result<(), Whatever> {
        let mut golden = self.golden.take().expect("foreground golden exists");
        let cancel = Arc::clone(&self.cancel_golden);
        self.golden_worker = Some(
            std::thread::Builder::new()
                .name("bemu-golden".to_string())
                .spawn(move || {
                    bebop_bemu::configure_default_topology();
                    while !golden.finished() && !cancel.load(Ordering::Relaxed) {
                        golden.step().map_err(|error| error.to_string())?;
                    }
                    if cancel.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    let code = golden.exit_code().unwrap_or(0);
                    if code != 0 {
                        return Err(format!("BEMU Golden Model exited with code {code}"));
                    }
                    Ok(())
                })
                .map_err(|error| Whatever::without_source(format!("failed to start BEMU Golden Model: {error}")))?,
        );
        Ok(())
    }

    pub fn finish_golden(&mut self) -> Result<(), Whatever> {
        if let Some(worker) = self.golden_worker.take() {
            return worker
                .join()
                .map_err(|_| Whatever::without_source("BEMU Golden Model worker panicked".to_string()))?
                .map_err(Whatever::without_source);
        }

        let golden = self.golden.as_mut().expect("foreground golden exists");
        while !golden.finished() {
            golden.step()?;
        }
        let code = golden.exit_code().unwrap_or(0);
        if code != 0 {
            return Err(Whatever::without_source(format!(
                "BEMU Golden Model exited with code {code}"
            )));
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<BankHashCompareSummary, Whatever> {
        let packet_status = runtime_packet_status();
        shutdown_runtime_packet_channel();
        let summary = self
            .compare_worker
            .take()
            .expect("DiffTest worker exists")
            .join()
            .map_err(|_| Whatever::without_source("Bank DiffTest M4 worker panicked".to_string()))?
            .map_err(Whatever::without_source)?;
        println!(
            "Bank DiffTest runtime packets: submitted={} no_sink={} send_failed={}",
            packet_status.submitted, packet_status.no_sink, packet_status.send_failed
        );
        Ok(summary)
    }
}

impl Drop for DiffSession {
    fn drop(&mut self) {
        self.cancel_golden.store(true, Ordering::Relaxed);
        shutdown_runtime_packet_channel();
        if let Some(worker) = self.golden_worker.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.compare_worker.take() {
            let _ = worker.join();
        }
    }
}
