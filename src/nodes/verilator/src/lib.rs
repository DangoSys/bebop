mod sim;

#[path = "../native/ffi.rs"]
mod ffi;

#[path = "trace/dpi.rs"]
mod dpi;

#[path = "trace/trace.rs"]
mod trace;

#[path = "mmio/mmio.rs"]
mod mmio;

pub use mmio::{drain_uart_tx, exit_code, push_uart_rx};
pub use sim::{setup_ctrlc_handler, should_exit, Simulator};
pub use trace::{init_trace, TraceConfig};
