/// Unknown instruction handler
/// Handles instructions that don't match known patterns

/// Process unknown instruction with default behavior
pub fn process(funct: u32, xs1: u64, xs2: u64) -> u64 {
    let sum = xs1.wrapping_add(xs2);
    println!("  -> Unknown funct={}, returning xs1+xs2=0x{:016x}", funct, sum);
    sum
}

