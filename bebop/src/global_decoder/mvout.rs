/// MVOUT (Move Out) instruction decoder
/// Moves data out of the accelerator scratchpad

/// Process MVOUT instruction
pub fn process(xs1: u64, xs2: u64) -> u64 {
  println!("  -> MVOUT: addr=0x{:016x}, config=0x{:016x}", xs1, xs2);
  // Return the address for now
  xs1
}
