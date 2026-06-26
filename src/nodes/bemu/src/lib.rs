mod sim;

#[path = "../native/ffi.rs"]
mod ffi;

#[path = "../native/spike.rs"]
mod spike;

#[path = "emu/bank/mod.rs"]
mod bank;

#[path = "emu/inst/mod.rs"]
mod inst;

mod trace;

pub use sim::BemuInstance;
pub use trace::TraceConfig;
