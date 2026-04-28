/// Simple UART (serial port) emulation
///
/// This provides a minimal UART implementation for console I/O

mod constants;
mod uart;

pub use constants::*;
pub use uart::*;
