/// MVIN (Move In) instruction decoder
/// Moves data into the accelerator scratchpad

/// Process MVIN instruction
pub fn process(xs1: u64, xs2: u64) -> u64 {
    println!("  -> MVIN: addr=0x{:016x}, config=0x{:016x}", xs1, xs2);
    // Return the address for now
    xs1
}

