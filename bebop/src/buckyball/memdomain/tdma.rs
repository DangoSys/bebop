use crate::buckyball::memdomain::banks::Banks;
use std::io::Result;

pub trait DmaInterface {
  fn dma_read(&self, addr: u64, size: u32) -> Result<u64>;
  fn dma_write(&self, addr: u64, data: u64, size: u32) -> Result<()>;
}

pub struct TDMA {}

impl TDMA {
  pub fn new() -> Self {
    Self {}
  }

  pub fn read<D: DmaInterface>(
    &mut self,
    addr: u64,
    vbank_id: u8,
    bank_index: u32,
    banks: &mut Banks,
    dma: &D,
  ) -> Result<u128> {
    // Use DMA to read data from external memory
    let size = 8; // 8 bytes for u64
    let data_u64 = dma.dma_read(addr, size)?;
    let data = data_u64 as u128;

    if !banks.write(vbank_id, bank_index, data) {
      return Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("Failed to write to bank {} at index {}", vbank_id, bank_index),
      ));
    }

    Ok(data)
  }

  pub fn write<D: DmaInterface>(
    &mut self,
    addr: u64,
    vbank_id: u8,
    bank_index: u32,
    banks: &Banks,
    dma: &D,
  ) -> Result<()> {
    let data = match banks.read(vbank_id, bank_index) {
      Some(d) => d,
      None => {
        return Err(std::io::Error::new(
          std::io::ErrorKind::InvalidInput,
          format!("Failed to read from bank {} at index {}", vbank_id, bank_index),
        ));
      },
    };

    // Use DMA to write data to external memory
    let size = 8; // 8 bytes for u64
    let data_u64 = data as u64;
    dma.dma_write(addr, data_u64, size)?;

    Ok(())
  }
}
