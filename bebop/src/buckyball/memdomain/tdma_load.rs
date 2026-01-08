use crate::buckyball::lib::operation::{ExternalOp, InternalOp};

pub trait DmaInterface {
  fn dma_read(&self, addr: u64, size: u32) -> std::io::Result<u128>;
}

pub struct TDMALoad {
  pub dma_banks_write_req: Option<(u32, u32, u128)>, // vbank_id, addr, data
  pub current_depth: u32,
  pub current_stride: u32,
  pub current_base_addr: u32,
  pub current_vbank_id: u32,
  pub current_index: u32,
  pub busy: bool,
}

impl TDMALoad {
  pub fn new() -> Self {
    Self {
      dma_banks_write_req: None,
      current_depth: 0,
      current_stride: 0,
      current_base_addr: 0,
      current_vbank_id: 0,
      current_index: 0,
      busy: false,
    }
  }

  pub fn mvin(&mut self) -> TDMALoadMvin {
    TDMALoadMvin(self)
  }
  pub fn dma_read_int<'a, D: DmaInterface>(&'a mut self, dma: &'a D) -> TDMALoadDmaReadInt<'a, D> {
    TDMALoadDmaReadInt(self, dma)
  }
}

/// ------------------------------------------------------------
/// --- Operations Definitions ---
/// ------------------------------------------------------------
pub struct TDMALoadMvin<'a>(&'a mut TDMALoad);
impl<'a> ExternalOp for TDMALoadMvin<'a> {
  type Input = Option<(u32, u32, u32, u32)>;

  fn can_input(&self, ctrl: bool) -> bool {
    ctrl && !self.0.busy
  }

  fn has_input(&self, input: &Self::Input) -> bool {
    input.is_some()
  }

  fn execute(&mut self, input: &Self::Input) {
    if !self.has_input(input) {
      return;
    }
    let (base_dram_addr, stride, depth, vbank_id) = input.unwrap();
    init_mvin(self.0, base_dram_addr, stride, depth, vbank_id);
  }
}

pub struct TDMALoadDmaReadInt<'a, D: DmaInterface>(&'a mut TDMALoad, &'a D);
impl<'a, D: DmaInterface> InternalOp for TDMALoadDmaReadInt<'a, D> {
  type Output = Option<(u32, u32, u128)>;

  fn has_output(&self) -> bool {
    self.0.dma_banks_write_req.is_some()
  }

  fn update(&mut self) {
    if self.0.current_index < self.0.current_depth {
      perform_dma_read(self.0, self.1);
    }
  }

  fn output(&mut self) -> Self::Output {
    if self.has_output() {
      let result = self.0.dma_banks_write_req;
      self.0.dma_banks_write_req = None;
      return result;
    }
    return None;
  }
}

/// ------------------------------------------------------------
/// --- Helper Functions ---
/// ------------------------------------------------------------
fn init_mvin(tdma: &mut TDMALoad, base_dram_addr: u32, stride: u32, depth: u32, vbank_id: u32) {
  tdma.current_base_addr = base_dram_addr;
  tdma.current_stride = stride;
  tdma.current_depth = depth;
  tdma.current_vbank_id = vbank_id;
  tdma.current_index = 0;
  tdma.busy = true;
  tdma.dma_banks_write_req = None;
}

fn perform_dma_read<D: DmaInterface>(tdma: &mut TDMALoad, dma: &D) {
  let addr = tdma.current_base_addr as u64 + (tdma.current_index as u64) * (tdma.current_stride as u64);
  match dma.dma_read(addr, 16) {
    Ok(data) => {
      tdma.dma_banks_write_req = Some((tdma.current_vbank_id, tdma.current_index, data));
      tdma.current_index += 1;
    },
    Err(e) => {
      eprintln!("DMA read failed at addr=0x{:x}: {:?}", addr, e);
      tdma.busy = false;
    },
  }
}

/// ------------------------------------------------------------
/// --- Test Functions ---
/// ------------------------------------------------------------
#[test]
fn test_tdma_load_init() {
  let mut tdma = TDMALoad::new();
  assert!(!tdma.busy);
  tdma.mvin().execute(&Some((0x1000, 16, 10, 0)));
  assert!(tdma.busy);
  assert_eq!(tdma.current_depth, 10);
}
