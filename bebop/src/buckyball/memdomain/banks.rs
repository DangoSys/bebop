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
        bank_width,
        bank_depth,
        bank_data: vec![0u128; bank_depth as usize],
      });
    }
    Self { banks }
  }

  pub fn exec_int(&mut self) -> Option<(u32, u64, u64)> {
    None
  }

  // Read from bank at specified index
  pub fn read(&self, vbank_id: u8, index: u32) -> Option<u128> {
    let bank_idx = vbank_id as usize;
    if bank_idx < self.banks.len() && (index as usize) < self.banks[bank_idx].bank_data.len() {
      Some(self.banks[bank_idx].bank_data[index as usize])
    } else {
      None
    }
  }

  // Write to bank at specified index
  pub fn write(&mut self, vbank_id: u8, index: u32, data: u128) -> bool {
    let bank_idx = vbank_id as usize;
    if bank_idx < self.banks.len() && (index as usize) < self.banks[bank_idx].bank_data.len() {
      self.banks[bank_idx].bank_data[index as usize] = data;
      true
    } else {
      false
    }
  }
}
