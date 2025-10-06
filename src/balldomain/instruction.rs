// Compute instructions for Ball Domain

#[derive(Debug, Clone, PartialEq)]
pub enum ComputeInstruction {
  Matmul { a_addr: u64, b_addr: u64, c_addr: u64, m: usize, n: usize, k: usize },
}

impl ComputeInstruction {
  pub fn parse(inst_str: &str) -> Result<Self, String> {
    let parts: Vec<&str> = inst_str.split_whitespace().collect();
    if parts.is_empty() {
      return Err("Empty instruction".to_string());
    }

    match parts[0] {
      "matmul" => {
        if parts.len() != 7 {
          return Err(format!("matmul expects 6 args, got {}", parts.len() - 1));
        }
        Ok(ComputeInstruction::Matmul {
          a_addr: Self::parse_addr(parts[1])?,
          b_addr: Self::parse_addr(parts[2])?,
          c_addr: Self::parse_addr(parts[3])?,
          m: Self::parse_usize(parts[4])?,
          n: Self::parse_usize(parts[5])?,
          k: Self::parse_usize(parts[6])?,
        })
      }
      _ => Err(format!("Unknown compute instruction: {}", parts[0])),
    }
  }

  fn parse_addr(s: &str) -> Result<u64, String> {
    let s = s.trim_start_matches("0x");
    u64::from_str_radix(s, 16)
      .map_err(|e| format!("Invalid address '{}': {}", s, e))
  }

  fn parse_usize(s: &str) -> Result<usize, String> {
    s.parse::<usize>()
      .map_err(|e| format!("Invalid number '{}': {}", s, e))
  }
}

