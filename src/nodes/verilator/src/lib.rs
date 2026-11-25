mod output;
mod sim;
mod threads;

#[path = "../native/ffi.rs"]
mod ffi;

#[path = "mmio/mmio.rs"]
mod mmio;

pub use bebop_rtl_trace::{init_trace, write_trace_summary, TraceConfig};
pub use mmio::{drain_uart_tx, exit_code, push_uart_rx};
pub use output::{captured_output, initialize_output};
pub use sim::Simulator;
pub use threads::HOST_THREADS;
