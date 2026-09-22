use std::io;
use std::path::Path;

use crate::state;

pub use crate::ctrace::ctrace;
pub use crate::itrace::{itrace, ITraceEvent};
pub use crate::mtrace::{mtrace, MTraceEvent};
pub use crate::pmctrace::{pmctrace_ball, pmctrace_mem};
pub use crate::state::{set_rtl_clk, TraceConfig};

pub fn init_trace(log_dir: &Path, config: TraceConfig) -> io::Result<()> {
    crate::dpi::force_link();
    state::init(log_dir, config)
}

pub fn write_trace_summary(log_dir: &Path) -> io::Result<()> {
    state::write_callback_summary(log_dir)
}
