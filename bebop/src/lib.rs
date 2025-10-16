/// Bebop - Accelerator simulator for RISC-V Spike
/// 
/// This library provides socket-based communication between Spike (RISC-V ISA simulator)
/// and custom accelerator implementations.

pub mod global_decoder;
pub mod simulator;
pub mod socket;

pub use simulator::Simulator;
pub use socket::{SocketMsg, SocketResp, SocketServer};

