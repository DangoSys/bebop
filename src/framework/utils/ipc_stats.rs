use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

static ON: AtomicBool = AtomicBool::new(true);

static W_IDLE_NS: AtomicU64 = AtomicU64::new(0);
static W_WORK_NS: AtomicU64 = AtomicU64::new(0);
static W_MEM_RPC_NS: AtomicU64 = AtomicU64::new(0);

pub fn set_on(v: bool) {
    ON.store(v, Ordering::Release);
}

pub fn on() -> bool {
    ON.load(Ordering::Acquire)
}

pub fn worker_idle(d: Duration) {
    if !on() {
        return;
    }
    W_IDLE_NS.fetch_add(d.as_nanos() as u64, Ordering::Relaxed);
}

pub fn worker_work(d: Duration) {
    if !on() {
        return;
    }
    W_WORK_NS.fetch_add(d.as_nanos() as u64, Ordering::Relaxed);
}

pub fn mem_rpc_wait(d: Duration) {
    if !on() {
        return;
    }
    W_MEM_RPC_NS.fetch_add(d.as_nanos() as u64, Ordering::Relaxed);
}

pub fn eprint_worker_summary(tag: &str) {
    if !on() {
        return;
    }
    let idle = W_IDLE_NS.load(Ordering::Relaxed);
    let work = W_WORK_NS.load(Ordering::Relaxed);
    let mem = W_MEM_RPC_NS.load(Ordering::Relaxed);
    let tot = idle.saturating_add(work);
    if tot == 0 {
        return;
    }
    let idle_pct = 100.0 * (idle as f64) / (tot as f64);
    let handle_pct = 100.0 * (work as f64) / (tot as f64);
    let mem_pct_wall = 100.0 * (mem as f64) / (tot as f64);
    let mem_pct_handle = if work > 0 {
        100.0 * (mem as f64) / (work as f64)
    } else {
        0.0
    };
    let pid = std::process::id();
    let line = format!(
        "[bebop ipc] worker pid={pid} role={tag}\n\
         | of worker wall: idle {idle_pct:.1}% | handle {handle_pct:.1}%\n\
         | mem wait (Spike MMU path): {mem_pct_wall:.1}% of wall | {mem_pct_handle:.1}% of handle\n",
    );
    let _ = std::io::stderr().lock().write_all(line.as_bytes());
}
