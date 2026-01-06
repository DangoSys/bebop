use crate::buckyball::memdomain::banks::Banks;
use crate::buckyball::memdomain::tdma::{DmaInterface, TDMA};

pub struct MemDomain {
  banks: Banks,
  tdma: TDMA,
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
  pub stride: u16,         // 10 bits: rs2[33:24]
  pub depth: u16,          // 16 bits: rs2[23:8]
  pub vbank_id: u8,        // 8 bits: rs2[7:0]
}

impl MemDomain {
  pub fn new() -> Self {
    // bank_num=16, bank_width=128, bank_depth=2048 (support depth up to 2048)
    Self {
      banks: Banks::new(16, 128, 2048),
      tdma: TDMA::new(),
    }
  }

  pub fn new_inst_ext<D: DmaInterface>(&mut self, received_inst: Option<(u32, u64, u64, u8, u32)>, dma: &D) -> bool {
    if received_inst.is_some() {
      let (funct, xs1, xs2, _domain_id, rob_id) = received_inst.unwrap();
      match decode_funct(funct, xs1, xs2) {
        Some(inst) => {
          println!(
            "MemDomain executed instruction: {:?}, base_dram_addr=0x{:x}, stride={}, depth={}, vbank_id={}, rob_id={}",
            inst.inst_type, inst.base_dram_addr, inst.stride, inst.depth, inst.vbank_id, rob_id
          );

          // Execute TDMA operations based on instruction type
          match inst.inst_type {
            MemInstType::Mvin => {
              // For mvin, read from DRAM and write to banks
              let base_addr = inst.base_dram_addr as u64;
              for i in 0..(inst.depth as u32) {
                let addr = base_addr + (i as u64) * (inst.stride as u64);
                let bank_index = i;
                if let Err(e) = self.tdma.read(addr, inst.vbank_id, bank_index, &mut self.banks, dma) {
                  eprintln!("TDMA read error: {}", e);
                  return false;
                }
              }
            },
            MemInstType::Mvout => {
              // For mvout, read from banks and write to DRAM
              let base_addr = inst.base_dram_addr as u64;
              for i in 0..(inst.depth as u32) {
                let addr = base_addr + (i as u64) * (inst.stride as u64);
                let bank_index = i;
                if let Err(e) = self.tdma.write(addr, inst.vbank_id, bank_index, &self.banks, dma) {
                  eprintln!("TDMA write error: {}", e);
                  return false;
                }
              }
            },
          }
        },
        None => {
          panic!(
            "Failed to decode instruction: funct={}, xs1={}, xs2={}",
            funct, xs1, xs2
          );
        },
      }
      return true;
    }
    return true;
  }

  pub fn execute_inst_int(&mut self) -> Option<(u32, u64, u64, u8, u32)> {
    None
  }
}

fn decode_funct(funct: u32, xs1: u64, xs2: u64) -> Option<MemInst> {
  // Check if this is mvin or mvout instruction
  let inst_type = match funct {
    24 => MemInstType::Mvin,
    25 => MemInstType::Mvout,
    _ => return None,
  };

  // Extract fields from rs1 (xs1)
  // base_dram_addr: bits [31:0] (32 bits)
  let base_dram_addr = (xs1 & 0xffffffff) as u32;

  // Extract fields from rs2 (xs2)
  // stride: bits [33:24] (10 bits)
  let stride = ((xs2 >> 24) & 0x3ff) as u16;

  // depth: bits [23:8] (16 bits)
  let depth = ((xs2 >> 8) & 0xffff) as u16;

  // vbank_id: bits [7:0] (8 bits)
  let vbank_id = (xs2 & 0xff) as u8;

  Some(MemInst {
    inst_type,
    base_dram_addr,
    stride,
    depth,
    vbank_id,
  })
}
