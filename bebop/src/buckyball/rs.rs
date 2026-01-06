
pub struct Rs {
  inst_to_issue: Option<(u32, u64, u64, u8, u32)>,
  }

impl Rs {
  pub fn new() -> Self {
    Self {
      inst_to_issue: None,
    }
  }
  
  pub fn inst_dispatch_ext(&mut self, received_inst: Option<(u32, u64, u64, u8, u32)>) -> bool {
    if received_inst.is_some() {
      let (funct, xs1, xs2, domain_id, rob_id) = received_inst.unwrap();
      self.inst_to_issue = Some((funct, xs1, xs2, domain_id, rob_id));
    } else {
      self.inst_to_issue = None;
    }
    true
  }

  pub fn issue_to_specific_domain_int(&mut self) -> Option<(u32, u64, u64, u8, u32)> {
    if self.inst_to_issue.is_some() {
      let (funct, xs1, xs2, domain_id, rob_id) = self.inst_to_issue.unwrap();
      println!("RS issue to specific domain: rob_id={:?}, funct={:?}, xs1={:?}, xs2={:?}, domain_id={:?}, rob_id={:?}", rob_id, funct, xs1, xs2, domain_id, rob_id);
      return Some((funct, xs1, xs2, domain_id, rob_id));
    }
    None
  }

}

