// Memory instructions for Mem Domain

#[derive(Debug, Clone, PartialEq)]
pub enum MemInstruction {
  Mvin { src_addr: u64, dst_addr: u64, size: usize },
  Mvout { src_addr: u64, dst_addr: u64, size: usize },
}

impl MemInstruction {
  pub fn parse(inst_str: &str) -> Result<Self, String> {
    let parts: Vec<&str> = inst_str.split_whitespace().collect();
    if parts.is_empty() {
      return Err("Empty instruction".to_string());
    }

    match parts[0] {
      "mvin" => {
        if parts.len() != 4 {
          return Err(format!("mvin expects 3 args, got {}", parts.len() - 1));
        }
        Ok(MemInstruction::Mvin {
          src_addr: Self::parse_addr(parts[1])?,
          dst_addr: Self::parse_addr(parts[2])?,
          size: Self::parse_usize(parts[3])?,
        })
      }
      "mvout" => {
        if parts.len() != 4 {
          return Err(format!("mvout expects 3 args, got {}", parts.len() - 1));
        }
        Ok(MemInstruction::Mvout {
          src_addr: Self::parse_addr(parts[1])?,
          dst_addr: Self::parse_addr(parts[2])?,
          size: Self::parse_usize(parts[3])?,
        })
      }
      _ => Err(format!("Unknown memory instruction: {}", parts[0])),
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

