/// Global instruction decoder for custom instructions
///
/// This module contains decoders for all custom accelerator instructions.
mod mvin;
mod mvout;

pub mod decoder;

use crate::socket::{SocketMsg, SocketResp};
pub use decoder::{Decoder, DecoderInput, DecoderOutput};

/// Decode and process a custom instruction (legacy function-based decoder)
pub fn decode_and_process(msg: &SocketMsg) -> SocketResp {
  // Copy fields to avoid packed struct alignment issues
  let funct = msg.funct;
  let xs1 = msg.xs1;
  let xs2 = msg.xs2;

  let result = match funct {
    0 => mvin::process(xs1, xs2),
    1 => mvout::process(xs1, xs2),
    _ => {
      println!("  -> UNKNOWN: funct={}, xs1=0x{:016x}, xs2=0x{:016x}", funct, xs1, xs2);
      0
    },
  };

  SocketResp::new(result)
}
