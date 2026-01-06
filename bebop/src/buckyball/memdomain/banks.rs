pub struct Bank {
  bank_id: u32,
  bank_width: u32,
  bank_depth: u32,
  bank_data: Vec<u128>,
}

pub struct Banks {
  banks: Vec<Bank>,
}

impl Banks {
  pub fn new(bank_num: u32, bank_width: u32, bank_depth: u32) -> Self {
    let mut banks = Vec::with_capacity(bank_num as usize);
    for i in 0..bank_num {
      banks.push(Bank {
        bank_id: i as u32,
        bank_width: bank_width,
        bank_depth: bank_depth,
        bank_data: Vec::with_capacity(bank_depth as usize),
      });
    }
    Self { banks }
  }
  
  pub fn exec_int(&mut self) -> Option<(u32, u64, u64)> {
    None
  }
}
