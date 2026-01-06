
pub struct Rs {
  rs_entry: Option<(u32, u32, u64, u64, u8, u32)>,
  }

impl Rs {
  pub fn new() -> Self {
    Self {
      rs_entry: None,
    }
  }
  
  pub fn rs_issue_ext(&mut self, rob_entry: Option<(u32, u32, u64, u64, u8)>) -> bool {
    if rob_entry.is_some() {
      let (rob_id, funct, xs1, xs2, domain_id) = rob_entry.unwrap();
      self.rs_entry = Some((rs_allocate_entry(), funct, xs1, xs2, domain_id, rob_id));
    } else {
      self.rs_entry = None;
    }
    true
  }

  pub fn rs_issue_int(&mut self) -> Option<(u32, u32, u64, u64, u8, u32)> {
    self.rs_entry
  }
}

fn rs_allocate_entry() -> u32 {
  println!("RS allocate entry");
  return 0;
}

fn rs_commit_entry(rs_entry: (u32, u32, u64, u64, u8, u32)) {
  println!("RS commit entry: {:?}", rs_entry);
}

fn rs_retire_entry(rs_entry: (u32, u32, u64, u64, u8, u32)) {
  println!("RS retire entry: {:?}", rs_entry);
}
