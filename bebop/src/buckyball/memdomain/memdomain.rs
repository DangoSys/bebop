pub struct MemDomain {}

impl MemDomain {
  pub fn new() -> Self {
    Self {}
  }

  pub fn new_inst_ext(&mut self, received_inst: Option<(u32, u64, u64, u8, u32)>) -> bool {
    if received_inst.is_some() {
      let (funct, xs1, xs2, domain_id, rob_id) = received_inst.unwrap();
      println!(
        "MemDomain executed instruction: funct={:?}, xs1={:?}, xs2={:?}, domain_id={:?}, rob_id={:?}",
        funct, xs1, xs2, domain_id, rob_id
      );
      return true;
    }
    return true;
  }

  pub fn execute_inst_int(&mut self) -> Option<(u32, u64, u64, u8, u32)> {
    None
  }
}
