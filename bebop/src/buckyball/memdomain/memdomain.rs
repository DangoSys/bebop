use crate::buckyball::lib::operation::{ExternalOp, InternalOp};
use crate::buckyball::memdomain::banks::Bank;
use crate::buckyball::memdomain::tdma_load::{DmaInterface, TDMALoad};
use crate::buckyball::memdomain::tdma_store::{DmaWriteInterface, TDMAStore};

pub struct MemDomain {
  banks: Vec<Bank>,
  tdma_load: TDMALoad,
  tdma_store: TDMAStore,

  received_inst: Option<(u32, u64, u64, u32)>,
  executed_inst: Option<(u32, u64, u64, u32)>,

  pub load_busy: bool,
  pub store_busy: bool,
}

#[derive(Debug)]
pub enum MemInstType {
  Mvin,
  Mvout,
}

#[derive(Debug)]
pub struct MemInst {
  pub inst_type: MemInstType,
  pub base_dram_addr: u32, // 32 bits: rs1[31:0]
  pub stride: u32,         // 10 bits: rs2[33:24]
  pub depth: u32,          // 16 bits: rs2[23:8]
  pub vbank_id: u32,       // 8 bits: rs2[7:0]
}

impl MemDomain {
  pub fn new() -> Self {
    let bank_num = 16;
    let bank_width = 128;
    let bank_depth = 2048;

    let mut banks = Vec::with_capacity(bank_num);
    for i in 0..bank_num {
      banks.push(Bank::new(i as u32, bank_width, bank_depth));
    }

    Self {
      banks,
      tdma_load: TDMALoad::new(),
      tdma_store: TDMAStore::new(),

      received_inst: None,
      executed_inst: None,

      load_busy: false,
      store_busy: false,
    }
  }

  pub fn load_op(&mut self) -> MemDomainLoadOp {
    MemDomainLoadOp(self)
  }

  pub fn store_op(&mut self) -> MemDomainStoreOp {
    MemDomainStoreOp(self)
  }

  pub fn execute_load<'a, D: DmaInterface + DmaWriteInterface>(
    &'a mut self,
    dma: &'a D,
  ) -> MemDomainExecuteLoadInt<'a, D> {
    MemDomainExecuteLoadInt(self, dma)
  }

  pub fn execute_store<'a, D: DmaInterface + DmaWriteInterface>(
    &'a mut self,
    dma: &'a D,
  ) -> MemDomainExecuteStoreInt<'a, D> {
    MemDomainExecuteStoreInt(self, dma)
  }
}

/// ------------------------------------------------------------
/// --- Operations Definitions ---
/// ------------------------------------------------------------
pub struct MemDomainLoadOp<'a>(&'a mut MemDomain);
impl<'a> ExternalOp for MemDomainLoadOp<'a> {
  type Input = Option<(u32, u64, u64, u32)>;

  fn can_input(&self, ctrl: bool) -> bool {
    ctrl && !self.0.load_busy
  }

  fn has_input(&self, input: &Self::Input) -> bool {
    input.is_some()
  }

  fn execute(&mut self, input: &Self::Input) {
    if !self.has_input(input) {
      return;
    }
    self.0.received_inst = *input;
    let (funct, xs1, xs2, _rob_id) = input.unwrap();
    if let Some(mem_config) = decode_funct(funct, xs1, xs2) {
      if let MemInstType::Mvin = mem_config.inst_type {
        self.0.tdma_load.mvin().execute(&Some((
          mem_config.base_dram_addr,
          mem_config.stride,
          mem_config.depth,
          mem_config.vbank_id,
        )));
        self.0.load_busy = true;
      }
    }
  }
}

pub struct MemDomainStoreOp<'a>(&'a mut MemDomain);
impl<'a> ExternalOp for MemDomainStoreOp<'a> {
  type Input = Option<(u32, u64, u64, u32)>;

  fn can_input(&self, ctrl: bool) -> bool {
    ctrl && !self.0.store_busy
  }

  fn has_input(&self, input: &Self::Input) -> bool {
    input.is_some()
  }

  fn execute(&mut self, input: &Self::Input) {
    if !self.has_input(input) {
      return;
    }
    self.0.received_inst = *input;
    let (funct, xs1, xs2, _rob_id) = input.unwrap();
    if let Some(mem_config) = decode_funct(funct, xs1, xs2) {
      if let MemInstType::Mvout = mem_config.inst_type {
        self.0.tdma_store.mvout().execute(&Some((
          mem_config.base_dram_addr,
          mem_config.stride,
          mem_config.depth,
          mem_config.vbank_id,
        )));
        self.0.store_busy = true;
      }
    }
  }
}

pub struct MemDomainExecuteLoadInt<'a, D: DmaInterface + DmaWriteInterface>(&'a mut MemDomain, &'a D);
impl<'a, D: DmaInterface + DmaWriteInterface> InternalOp for MemDomainExecuteLoadInt<'a, D> {
  type Output = Option<(u32, u64, u64, u32)>;

  fn has_output(&self) -> bool {
    self.0.executed_inst.is_some()
  }

  fn update(&mut self) {
    if self.0.load_busy {
      let mut dma_read = self.0.tdma_load.dma_read_int(self.1);
      dma_read.update();
      if let Some((vbank_id, addr, data)) = dma_read.output() {
        let bank_idx = vbank_id as usize;
        assert!(bank_idx < self.0.banks.len());
        self.0.banks[bank_idx].write_req().execute(&Some((addr, data)));
      }
      if !self.0.tdma_load.busy {
        self.0.load_busy = false;
        if let Some((funct, xs1, xs2, rob_id)) = self.0.received_inst {
          self.0.executed_inst = Some((funct, xs1, xs2, rob_id));
        }
      }
    }
  }

  fn output(&mut self) -> Self::Output {
    if self.has_output() {
      let result = self.0.executed_inst;
      self.0.executed_inst = None;
      return result;
    }
    return None;
  }
}

pub struct MemDomainExecuteStoreInt<'a, D: DmaInterface + DmaWriteInterface>(&'a mut MemDomain, &'a D);
impl<'a, D: DmaInterface + DmaWriteInterface> InternalOp for MemDomainExecuteStoreInt<'a, D> {
  type Output = Option<(u32, u64, u64, u32)>;

  fn has_output(&self) -> bool {
    self.0.executed_inst.is_some()
  }

  fn update(&mut self) {
    if self.0.store_busy {
      // 第一步：处理 bank 读请求
      if let Some((vbank_id, addr)) = self.0.tdma_store.dma_banks_read_req {
        let bank_idx = vbank_id as usize;
        assert!(bank_idx < self.0.banks.len());
        self.0.banks[bank_idx].read_req().execute(&Some(addr));
        self.0.tdma_store.dma_banks_read_req = None;
      }

      // 第二步：检查对应的 bank 是否有读响应
      let vbank_id = self.0.tdma_store.current_vbank_id;
      let bank_idx = vbank_id as usize;
      if bank_idx < self.0.banks.len() {
        let mut read_resp = self.0.banks[bank_idx].read_resp();
        read_resp.update();
        if let Some(data) = read_resp.output() {
          let dram_addr = self.0.tdma_store.current_base_addr as u64
            + (self.0.tdma_store.current_index as u64) * (self.0.tdma_store.current_stride as u64);
          self.0.tdma_store.dma_write_req = Some((dram_addr, data));
        }
      }

      // 第三步：执行 DMA 写入
      let mut dma_write = self.0.tdma_store.dma_write_int(self.1);
      dma_write.update();
      if dma_write.output() {
        self.0.store_busy = false;
        if let Some((funct, xs1, xs2, rob_id)) = self.0.received_inst {
          self.0.executed_inst = Some((funct, xs1, xs2, rob_id));
        }
      }
    }
  }

  fn output(&mut self) -> Self::Output {
    if self.has_output() {
      let result = self.0.executed_inst;
      self.0.executed_inst = None;
      return result;
    }
    return None;
  }
}

/// ------------------------------------------------------------
/// --- Helper Functions ---
/// ------------------------------------------------------------
fn decode_funct(funct: u32, xs1: u64, xs2: u64) -> Option<MemInst> {
  let inst_type = match funct {
    24 => MemInstType::Mvin,
    25 => MemInstType::Mvout,
    _ => return None,
  };
  let base_dram_addr = (xs1 & 0xffffffff) as u32;
  let stride = ((xs2 >> 24) & 0x3ff) as u32;
  let depth = ((xs2 >> 8) & 0xffff) as u32;
  let vbank_id = (xs2 & 0xff) as u32;
  Some(MemInst {
    inst_type,
    base_dram_addr,
    stride,
    depth,
    vbank_id,
  })
}
