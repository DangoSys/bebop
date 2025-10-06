// Ball Domain Decoder: decodes compute instructions

use super::instruction::ComputeInstruction;

pub struct BallDomainDecoder;

impl BallDomainDecoder {
  pub fn new() -> Self {
    Self
  }

  pub fn decode(&self, inst_str: &str) -> Result<ComputeInstruction, String> {
    let parts: Vec<&str> = inst_str.split_whitespace().collect();
    if parts.is_empty() {
      return Err("Empty instruction".to_string());
    }

    match parts[0] {
      "matmul" => {
        if parts.len() != 7 {
          return Err(format!("matmul expects 6 args, got {}", parts.len() - 1));
        }
        println!("[BallDomainDecoder] Decoded matmul instruction");
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

