mod sim;

#[path = "../native/ffi.rs"]
mod ffi;

#[path = "../native/spike.rs"]
mod spike;

#[path = "emu/bank/mod.rs"]
mod bank;

#[path = "emu/config.rs"]
mod config;

#[path = "emu/inst/mod.rs"]
mod inst;

mod trace;

pub use sim::BemuInstance;
pub use trace::TraceConfig;

/// Private-bank geometry used by an in-process RTL DiffTest monitor. Keeping
/// this query in the chip wrapper ensures it follows that wrapper's
/// `BEMU_TOP_CONFIG` rather than a hard-coded topology.
pub fn private_bank_geometry() -> (usize, usize) {
    (config::bank_size(), config::bank_row_bytes())
}
