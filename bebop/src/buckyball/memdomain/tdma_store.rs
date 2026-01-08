use crate::buckyball::lib::operation::{ExternalOp, InternalOp};

pub trait DmaWriteInterface {
  fn dma_write(&self, addr: u64, data: u128, size: u32) -> std::io::Result<()>;
}

pub struct TDMAStore {
  pub dma_banks_read_req: Option<(u32, u32)>,
  pub dma_write_req: Option<(u64, u128)>,
  pub current_depth: u32,
  pub current_stride: u32,
  pub current_base_addr: u32,
  pub current_vbank_id: u32,
  pub current_index: u32,
  pub busy: bool,
}

impl TDMAStore {
  pub fn new() -> Self {
    Self {
      dma_banks_read_req: None,
      dma_write_req: None,
      current_depth: 0,
      current_stride: 0,
      current_base_addr: 0,
      current_vbank_id: 0,
      current_index: 0,
      busy: false,
    }
  }

  pub fn mvout(&mut self) -> TDMAStoreMvout {
    TDMAStoreMvout(self)
  }
  pub fn dma_write_int<'a, D: DmaWriteInterface>(&'a mut self, dma: &'a D) -> TDMAStoreDmaWriteInt<'a, D> {
    TDMAStoreDmaWriteInt(self, dma)
  }
}

/// ------------------------------------------------------------
/// --- Operations Definitions ---
/// ------------------------------------------------------------
pub struct TDMAStoreMvout<'a>(&'a mut TDMAStore);
impl<'a> ExternalOp for TDMAStoreMvout<'a> {
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
    init_mvout(self.0, base_dram_addr, stride, depth, vbank_id);
  }
}

pub struct TDMAStoreDmaWriteInt<'a, D: DmaWriteInterface>(&'a mut TDMAStore, &'a D);
impl<'a, D: DmaWriteInterface> InternalOp for TDMAStoreDmaWriteInt<'a, D> {
  type Output = bool;

  fn has_output(&self) -> bool {
    self.0.current_index >= self.0.current_depth
  }

  fn update(&mut self) {
    // 向bank请求读数据
    if self.0.current_index < self.0.current_depth {
      self.0.dma_banks_read_req = Some((self.0.current_vbank_id, self.0.current_index));
    }
    // 写数据到dram
    if let Some((dram_addr, data)) = self.0.dma_write_req.take() {
      perform_dma_write(self.0, self.1, dram_addr, data);
    }
  }

  fn output(&mut self) -> Self::Output {
    if self.has_output() {
      self.0.busy = false;
      return true;
    }
    return false;
  }
}

/// ------------------------------------------------------------
/// --- Helper Functions ---
/// ------------------------------------------------------------
fn init_mvout(tdma: &mut TDMAStore, base_dram_addr: u32, stride: u32, depth: u32, vbank_id: u32) {
  tdma.current_base_addr = base_dram_addr;
  tdma.current_stride = stride;
  tdma.current_depth = depth;
  tdma.current_vbank_id = vbank_id;
  tdma.current_index = 0;
  tdma.busy = true;
  tdma.dma_banks_read_req = None;
  tdma.dma_write_req = None;
}

fn perform_dma_write<D: DmaWriteInterface>(tdma: &mut TDMAStore, dma: &D, dram_addr: u64, data: u128) {
  match dma.dma_write(dram_addr, data, 16) {
    Ok(_) => {
      tdma.current_index += 1;
      if tdma.current_index >= tdma.current_depth {
        tdma.busy = false;
      }
    },
    Err(e) => {
      eprintln!("DMA write failed at addr=0x{:x}: {:?}", dram_addr, e);
      tdma.busy = false;
    },
  }
}

/// ------------------------------------------------------------
/// --- Test Functions ---
/// ------------------------------------------------------------
#[test]
fn test_tdma_store_init() {
  let mut tdma = TDMAStore::new();
  assert!(!tdma.busy);
  tdma.mvout().execute(&Some((0x2000, 16, 5, 1)));
  assert!(tdma.busy);
  assert_eq!(tdma.current_depth, 5);
}
