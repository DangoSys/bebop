
pub struct Decoder {
  decoded_inst: Option<(u32, u64, u64, u8)>,
}

impl Decoder {
  pub fn new() -> Self {
    Self {
      decoded_inst: None,
    }
  }

  pub fn inst_decode_ext(&mut self, raw_inst: Option<(u32, u64, u64)>) -> bool {
    if raw_inst.is_some() {
      let (funct, xs1, xs2) = raw_inst.unwrap();
      self.decoded_inst = Some((funct, xs1, xs2, decode_funct(funct)));
    } else {
      self.decoded_inst = None;
    }
    true
  }

  pub fn push_to_rob_int(&mut self) -> Option<(u32, u64, u64, u8)> {
    self.decoded_inst
  }
}

fn decode_funct(funct: u32) -> u8 {
  let domain_id = match funct {
    31 => 0,      // Fence -> domain 0
    24 | 25 => 1, // Load -> domain 1 (memdomain)
    _ => 2,       // Compute -> domain 2 (balldomain),
  };
  println!("Decoded domain: {:?}", domain_id);
  domain_id
}

#[test]
fn test_decode_funct() {
  assert_eq!(decode_funct(31), 0);
  assert_eq!(decode_funct(24), 1);
  assert_eq!(decode_funct(26), 2);
}
