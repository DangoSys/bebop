/// Decode funct to domain id
pub fn decode_funct(funct: u32) -> u8 {
  match funct {
    31 => 0,      // Fence -> domain 0
    24 | 25 => 1, // Load -> domain 1 (memdomain)
    _ => 2,       // Compute -> domain 2 (balldomain)
  }
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
