/// MVIN (Move In) instruction decoder
/// Moves data into the accelerator scratchpad via DMA
///
/// This instruction triggers a DMA read from DRAM to scratchpad memory.
/// xs1: DRAM address to read from
/// xs2: Configuration (bits [63:32] = scratchpad addr, bits [31:0] = count)

/// Process MVIN instruction
pub fn process(xs1: u64, xs2: u64) -> u64 {
  println!("  -> MVIN: dram_addr=0x{:016x}, config=0x{:016x}", xs1, xs2);

  // Parse config
  let spad_addr = (xs2 >> 32) & 0xFFFFFFFF;
  let count = xs2 & 0xFFFFFFFF;

  println!(
    "     Read {} words from DRAM 0x{:x} to scratchpad addr 0x{:x}",
    count, xs1, spad_addr
  );

  // DMA operation will be handled by simulator
  0
}
