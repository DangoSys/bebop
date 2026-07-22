use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

static TRACE_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static ENABLE_ITRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static ENABLE_MTRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static ENABLE_PMCTRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static ENABLE_CTRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static ENABLE_BANKTRACE: OnceLock<Mutex<bool>> = OnceLock::new();
static RTL_CLK: OnceLock<Mutex<u64>> = OnceLock::new();
static ITRACE_CALLBACKS: AtomicU64 = AtomicU64::new(0);
static MTRACE_CALLBACKS: AtomicU64 = AtomicU64::new(0);
static PMC_BALL_CALLBACKS: AtomicU64 = AtomicU64::new(0);
static PMC_MEM_CALLBACKS: AtomicU64 = AtomicU64::new(0);
static CTRACE_CALLBACKS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default)]
pub struct TraceConfig {
    pub itrace: bool,
    pub mtrace: bool,
    pub pmctrace: bool,
    pub ctrace: bool,
    pub banktrace: bool,
}

pub fn init(log_dir: &Path, config: TraceConfig) -> io::Result<()> {
    std::fs::create_dir_all(log_dir)?;
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(log_dir.join("bdb.ndjson"))?;

    *trace_file().lock().unwrap() = Some(file);
    *enable_itrace().lock().unwrap() = config.itrace;
    *enable_mtrace().lock().unwrap() = config.mtrace;
    *enable_pmctrace().lock().unwrap() = config.pmctrace;
    *enable_ctrace().lock().unwrap() = config.ctrace;
    *enable_banktrace().lock().unwrap() = config.banktrace;
    ITRACE_CALLBACKS.store(0, Ordering::Relaxed);
    MTRACE_CALLBACKS.store(0, Ordering::Relaxed);
    PMC_BALL_CALLBACKS.store(0, Ordering::Relaxed);
    PMC_MEM_CALLBACKS.store(0, Ordering::Relaxed);
    CTRACE_CALLBACKS.store(0, Ordering::Relaxed);

    Ok(())
}

pub fn set_rtl_clk(clk: u64) {
    *rtl_clk_state().lock().unwrap() = clk;
}

pub fn rtl_clk() -> u64 {
    *rtl_clk_state().lock().unwrap()
}

pub fn itrace_enabled() -> bool {
    *enable_itrace().lock().unwrap()
}

pub fn mtrace_enabled() -> bool {
    *enable_mtrace().lock().unwrap()
}

pub fn pmctrace_enabled() -> bool {
    *enable_pmctrace().lock().unwrap()
}

pub fn ctrace_enabled() -> bool {
    *enable_ctrace().lock().unwrap()
}

pub fn banktrace_enabled() -> bool {
    *enable_banktrace().lock().unwrap()
}

pub fn write_trace(json: &str) {
    if let Some(ref mut file) = *trace_file().lock().unwrap() {
        writeln!(file, "{json}").ok();
        file.flush().ok();
    }
}

pub fn record_itrace_callback() {
    ITRACE_CALLBACKS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_mtrace_callback() {
    MTRACE_CALLBACKS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_pmc_ball_callback() {
    PMC_BALL_CALLBACKS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_pmc_mem_callback() {
    PMC_MEM_CALLBACKS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_ctrace_callback() {
    CTRACE_CALLBACKS.fetch_add(1, Ordering::Relaxed);
}

pub fn write_callback_summary(log_dir: &Path) -> io::Result<()> {
    let json = format!(
        concat!(
            "{{\"itrace_callbacks\":{},\"mtrace_callbacks\":{},",
            "\"pmctrace_ball_callbacks\":{},\"pmctrace_mem_callbacks\":{},",
            "\"ctrace_callbacks\":{}}}\n"
        ),
        ITRACE_CALLBACKS.load(Ordering::Relaxed),
        MTRACE_CALLBACKS.load(Ordering::Relaxed),
        PMC_BALL_CALLBACKS.load(Ordering::Relaxed),
        PMC_MEM_CALLBACKS.load(Ordering::Relaxed),
        CTRACE_CALLBACKS.load(Ordering::Relaxed),
    );
    std::fs::write(log_dir.join("rtl-trace-summary.json"), json)
}

fn trace_file() -> &'static Mutex<Option<File>> {
    TRACE_FILE.get_or_init(|| Mutex::new(None))
}

fn rtl_clk_state() -> &'static Mutex<u64> {
    RTL_CLK.get_or_init(|| Mutex::new(0))
}

fn enable_itrace() -> &'static Mutex<bool> {
    ENABLE_ITRACE.get_or_init(|| Mutex::new(false))
}

fn enable_mtrace() -> &'static Mutex<bool> {
    ENABLE_MTRACE.get_or_init(|| Mutex::new(false))
}

fn enable_pmctrace() -> &'static Mutex<bool> {
    ENABLE_PMCTRACE.get_or_init(|| Mutex::new(false))
}

fn enable_ctrace() -> &'static Mutex<bool> {
    ENABLE_CTRACE.get_or_init(|| Mutex::new(false))
}

fn enable_banktrace() -> &'static Mutex<bool> {
    ENABLE_BANKTRACE.get_or_init(|| Mutex::new(false))
}
