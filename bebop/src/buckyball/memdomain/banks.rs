use crate::buckyball::lib::operation::{ExternalOp, InternalOp};

pub struct Bank {
  bank_id: u32,
  bank_width: u32,
  bank_depth: u32,
  bank_data: Vec<u128>,

  read_resp: Option<u128>, // data
}

impl Bank {
  pub fn new(bank_id: u32, bank_width: u32, bank_depth: u32) -> Self {
    Self {
      bank_id,
      bank_width,
      bank_depth,
      bank_data: vec![0u128; bank_depth as usize],
      read_resp: None,
    }
  }

  pub fn read_req(&mut self) -> BankReadReq {
    BankReadReq(self)
  }

  pub fn write_req(&mut self) -> BankWriteReq {
    BankWriteReq(self)
  }

  pub fn read_resp(&mut self) -> BankReadResp {
    BankReadResp(self)
  }
}

/// ------------------------------------------------------------
/// --- Operations Definitions ---
/// ------------------------------------------------------------
pub struct BankReadReq<'a>(&'a mut Bank);
impl<'a> ExternalOp for BankReadReq<'a> {
  type Input = Option<u32>; // addr

  fn can_input(&self, ctrl: bool) -> bool {
    ctrl && true
  }

  fn has_input(&self, input: &Self::Input) -> bool {
    input.is_some()
  }

  fn execute(&mut self, input: &Self::Input) {
    if !self.has_input(input) {
      return;
    }
    let addr = input.unwrap();
    self.0.read_resp = read_data(&mut self.0, addr);
  }
}

pub struct BankWriteReq<'a>(&'a mut Bank);
impl<'a> ExternalOp for BankWriteReq<'a> {
  type Input = Option<(u32, u128)>; // addr, data

  fn can_input(&self, ctrl: bool) -> bool {
    ctrl && true
  }

  fn has_input(&self, input: &Self::Input) -> bool {
    input.is_some()
  }

  fn execute(&mut self, input: &Self::Input) {
    if !self.has_input(input) {
      return;
    }
    let (addr, data) = input.unwrap();
    write_data(&mut self.0, addr, data);
  }
}

pub struct BankReadResp<'a>(&'a mut Bank);
impl<'a> InternalOp for BankReadResp<'a> {
  type Output = Option<u128>;

  fn has_output(&self) -> bool {
    self.0.read_resp.is_some()
  }

  fn update(&mut self) {}

  fn output(&mut self) -> Self::Output {
    self.0.read_resp.take()
  }
}

/// ------------------------------------------------------------
/// --- Helper Functions ---
/// ------------------------------------------------------------
fn read_data(bank: &mut Bank, addr: u32) -> Option<u128> {
  assert!((addr as usize) < bank.bank_data.len());
  Some(bank.bank_data[addr as usize])
}

fn write_data(bank: &mut Bank, addr: u32, data: u128) {
  assert!((addr as usize) < bank.bank_data.len());
  bank.bank_data[addr as usize] = data;
}

/// ------------------------------------------------------------
/// --- Test Functions ---
/// ------------------------------------------------------------
#[test]
fn test_bank_read_write() {
  let mut bank = Bank::new(0, 128, 1024);
  bank.write_req().execute(&Some((10, 0x1234)));
  bank.read_req().execute(&Some(10));
  let data = bank.read_resp().output();
  assert_eq!(data, Some(0x1234));
}
