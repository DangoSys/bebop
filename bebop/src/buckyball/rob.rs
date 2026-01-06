
pub struct Rob {
  rob_entry: Option<(u32, u32, u64, u64, u8)>,
}

impl Rob {
  pub fn new() -> Self {
    Self {
      rob_entry: None,
    }
  }

  pub fn rob_allocate(&mut self, decoded_inst: Option<(u32, u64, u64, u8)>) {
    if decoded_inst.is_some() {
      let (funct, xs1, xs2, domain_id) = decoded_inst.unwrap();
      self.rob_entry = Some((rob_allocate_entry(), funct, xs1, xs2, domain_id));
    } else {
      self.rob_entry = None;
    }
  } 

  pub fn rob_allocate_bw(&mut self) -> Option<(u32, u32, u64, u64, u8)> {
    self.rob_entry
  }
}

fn rob_allocate_entry() -> u32 {
    println!("ROB allocate entry");
    return 0;
}   
