/// Global instruction decoder for custom instructions
/// 
/// This module contains decoders for all custom accelerator instructions.
/// Each instruction type is handled by a separate submodule for better organization.

mod mvin;
mod mvout;
mod compute;
mod config;
mod unknown;

use crate::socket::{SocketMsg, SocketResp};

/// Decode and process a custom instruction
pub fn decode_and_process(msg: &SocketMsg) -> SocketResp {
    // Copy fields to avoid packed struct alignment issues
    let funct = msg.funct;
    let xs1 = msg.xs1;
    let xs2 = msg.xs2;
    
    let result = match funct {
        0 => mvin::process(xs1, xs2),
        1 => mvout::process(xs1, xs2),
        2 => compute::process(xs1, xs2),
        4 => config::process(xs1, xs2),
        _ => unknown::process(funct, xs1, xs2),
    };

    SocketResp::new(result)
}

