/// Socket communication module for Spike-Bebop interface
mod handler;
mod protocol;
mod server;

pub use protocol::{SocketMsg, SocketResp};
pub use server::SocketServer;
