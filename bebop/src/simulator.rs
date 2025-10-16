/// Accelerator simulator with state management
use crate::global_decoder;
use crate::socket::{SocketMsg, SocketResp};

/// Accelerator simulator that manages state
pub struct Simulator {
    // Future: add scratchpad memory, configuration state, etc.
    // scratchpad: Vec<u8>,
    // config: AcceleratorConfig,
}

impl Simulator {
    pub fn new() -> Self {
        Self {}
    }

    /// Process an instruction (delegates to global decoder for now)
    pub fn process(&mut self, msg: &SocketMsg) -> SocketResp {
        // Currently stateless, delegates to global decoder
        // Future: manage state before/after instruction processing
        global_decoder::decode_and_process(msg)
    }
}

impl Default for Simulator {
    fn default() -> Self {
        Self::new()
    }
}

