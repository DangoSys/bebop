mod banktrace;
mod ctrace;
mod dpi;
mod itrace;
mod mtrace;
mod pmctrace;
mod state;
mod trace;

pub use trace::{init_trace, set_rtl_clk, write_trace_summary, TraceConfig};
