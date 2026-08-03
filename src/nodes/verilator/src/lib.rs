mod sim;

#[path = "../native/ffi.rs"]
mod ffi;

#[path = "mmio/mmio.rs"]
mod mmio;

pub use bebop_rtl_trace::{finish_bank_digest, init_trace, poll_bank_digest, BankDigestConfig, TraceConfig};
pub use mmio::{drain_uart_tx, exit_code, push_uart_rx};
pub use sim::{setup_ctrlc_handler, should_exit, Simulator};
