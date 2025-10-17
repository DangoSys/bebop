/// MVOUT (Move Out) instruction decoder
/// Moves data out of the accelerator scratchpad via DMA
///
/// This instruction triggers a DMA write from scratchpad to DRAM.
/// xs1: DRAM address to write to
/// xs2: Configuration (bits [63:32] = scratchpad addr, bits [31:0] = count)

/// Process MVOUT instruction
pub fn process(xs1: u64, xs2: u64) -> u64 {
  println!("  -> MVOUT: dram_addr=0x{:016x}, config=0x{:016x}", xs1, xs2);

  // Parse config
  let spad_addr = (xs2 >> 32) & 0xFFFFFFFF;
  let count = xs2 & 0xFFFFFFFF;

  println!(
    "     Write {} words from scratchpad addr 0x{:x} to DRAM 0x{:x}",
    count, spad_addr, xs1
  );

  // DMA operation will be handled by simulator
  0
}
