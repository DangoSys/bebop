pub struct DomainDecoder {
  decoded_inst: Option<(u32, u64, u64, u32, u32)>,
}

impl DomainDecoder {
  pub fn new() -> Self {
    Self { decoded_inst: None }
  }

  pub fn new_inst_ext(&mut self, raw_inst: Option<(u32, u64, u64, u32)>) -> bool {
    if raw_inst.is_some() {
      let (funct, xs1, xs2, rob_id) = raw_inst.unwrap();
      self.decoded_inst = Some((funct, xs1, xs2, rob_id, decode_funct(funct)));
    } else {
      self.decoded_inst = None;
    }
    true
  }

  pub fn exec_int(&mut self) -> Option<(u32, u64, u64, u32, u32)> {
    if self.decoded_inst.is_some() {
      return self.decoded_inst;
    }
    None
  }
}

fn decode_funct(funct: u32) -> u32 {
  let ball_id = match funct {
    27 => 0, // VectorBall
    28 => 1, // TransposeBall
    29 => 2, // ReluBall
    _ => panic!("Invalid funct: {:?}", funct),
  };
  println!("Decoded ball: {:?}", ball_id);
  ball_id
}
