pub struct TransBall {
  received_inst: Option<(u32, u64, u64, u32)>,
}

impl TransBall {
  pub fn new() -> Self {
    Self { received_inst: None }
  }

  pub fn new_inst_ext(&mut self, received_inst: Option<(u32, u64, u64, u32)>) -> bool {
    if received_inst.is_some() {
      let (funct, xs1, xs2, rob_id) = received_inst.unwrap();
      self.received_inst = Some((funct, xs1, xs2, rob_id));
    } else {
      self.received_inst = None;
    }
    true
  }

  pub fn exec_int(&mut self) -> Option<(u32, u64, u64)> {
    if self.received_inst.is_some() {
      let (funct, xs1, xs2, rob_id) = self.received_inst.unwrap();
      println!(
        "TransBall executed instruction: rob_id={:?}, funct={:?}, xs1={:?}, xs2={:?}",
        rob_id, funct, xs1, xs2
      );
      return Some((funct, xs1, xs2));
    }
    None
  }
}
