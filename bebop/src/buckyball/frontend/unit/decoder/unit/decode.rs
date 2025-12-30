use crate::buckyball::frontend::bundles::rocc_frontend::RoccInstruction;
use crate::buckyball::frontend::unit::rob::bundles::decoder_rob::DecodedInstruction;


pub fn decode_funct(funct: u32) -> u8 {
  match funct {
    31 => 0,      // Fence -> domain 0
    24 | 25 => 1, // Load -> domain 1 (memdomain)
    _ => 2,       // Compute -> domain 2 (balldomain)
  }
}

pub fn decode_instruction(raw_inst: RoccInstruction) -> DecodedInstruction {
  DecodedInstruction::new(raw_inst.funct, raw_inst.xs1, raw_inst.xs2, decode_funct(raw_inst.funct))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_decode_funct() {
    assert_eq!(decode_funct(24), 1);
    assert_eq!(decode_funct(31), 0);
  }
}
