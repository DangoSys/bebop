use crate::buckyball::lib::operation::{ExternalOp, InternalOp};
use std::sync::atomic::{AtomicBool, Ordering};
pub static FENCE_CSR: AtomicBool = AtomicBool::new(false);

pub struct Decoder {
  decoded_inst: Option<(u32, u64, u64, u32)>,
}

impl Decoder {
  pub fn new() -> Self {
    Self { decoded_inst: None }
  }

  pub fn decode(&mut self) -> DecoderDecode {
    DecoderDecode(self)
  }
  pub fn push_to_rob(&mut self) -> DecoderPushToRob {
    DecoderPushToRob(self)
  }
}

/// ------------------------------------------------------------
/// --- Operations Definitions ---
/// ------------------------------------------------------------
pub struct DecoderDecode<'a>(&'a mut Decoder);
impl<'a> ExternalOp for DecoderDecode<'a> {
  type Input = Option<(u32, u64, u64)>;
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
    let (funct, xs1, xs2) = input.unwrap();
    let domain_id = decode_funct(funct);
    // fence instruction should not be pushed to ROB
    if domain_id == 0 {
      self.0.decoded_inst = None;
      return;
    }
    self.0.decoded_inst = Some((funct, xs1, xs2, domain_id));
    println!("[Decoder] Decoded instruction: funct={:?}", funct);
  }
}

pub struct DecoderPushToRob<'a>(&'a mut Decoder);
impl<'a> InternalOp for DecoderPushToRob<'a> {
  type Output = Option<(u32, u64, u64, u32)>;

  fn has_output(&self) -> bool {
    self.0.decoded_inst.is_some()
  }

  fn update(&mut self) { }

  fn output(&mut self) -> Self::Output {
    if self.has_output() {
      println!("[Decoder] Pushed to ROB: {:?}", self.0.decoded_inst);
      return self.0.decoded_inst;
    }
    return None;
  }
}

/// ------------------------------------------------------------
/// --- Helper Functions ---
/// ------------------------------------------------------------
fn decode_funct(funct: u32) -> u32 {
  let domain_id = match funct {
    31 => 0,      // Fence -> domain 0
    24 | 25 => 1, // Load -> domain 1 (memdomain)
    _ => 2,       // Compute -> domain 2 (balldomain),
  };
  domain_id
}

/// ------------------------------------------------------------
/// --- Test Functions ---
/// ------------------------------------------------------------
#[test]
fn test_decode_funct() {
  assert_eq!(decode_funct(31), 0);
  assert_eq!(decode_funct(24), 1);
  assert_eq!(decode_funct(26), 2);
}
