// Global Decoder: decodes instruction type (mem or compute)

#[derive(Debug, Clone, PartialEq)]
pub enum InstructionType {
  Mem,
  Compute,
}

pub struct GlobalDecoder;

impl GlobalDecoder {
  pub fn new() -> Self {
    Self
  }

  pub fn decode(&self, inst_str: &str) -> Result<InstructionType, String> {
    let parts: Vec<&str> = inst_str.split_whitespace().collect();
    if parts.is_empty() {
      return Err("Empty instruction".to_string());
    }

    match parts[0] {
      "mvin" | "mvout" => {
        println!("[GlobalDecoder] Decoded as Mem instruction");
        Ok(InstructionType::Mem)
      }
      "matmul" => {
        println!("[GlobalDecoder] Decoded as Compute instruction");
        Ok(InstructionType::Compute)
      }
      _ => Err(format!("Unknown instruction type: {}", parts[0])),
    }
  }
}

