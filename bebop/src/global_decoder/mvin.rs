/// MVIN (Move In) instruction decoder
/// Moves data into the accelerator scratchpad via DMA
///
/// This instruction triggers a DMA read from DRAM to scratchpad memory.
/// 
/// Instruction format (from bb-tests/workloads/lib/bbhw/isa/24_mvin.c):
/// rs1[0:31]: base_dram_addr - DRAM起始地址
/// rs2[0:13]: base_sp_addr - scratchpad起始地址
/// rs2[14:23]: iter - 迭代次数
/// rs2[24:33]: col_stride - 列跨度

/// MVIN 指令配置
#[derive(Debug, Clone)]
pub struct MvinConfig {
  pub base_dram_addr: u32,  // DRAM起始地址
  pub base_sp_addr: u32,     // scratchpad起始地址 (14位)
  pub iter: u32,             // 迭代次数 (10位)
  pub col_stride: u32,       // 列跨度 (10位)
}

impl MvinConfig {
  /// 从指令字段解析配置
  pub fn from_fields(xs1: u64, xs2: u64) -> Self {
    // rs1[0:31]: base_dram_addr
    let base_dram_addr = (xs1 & 0xFFFFFFFF) as u32;
    
    // rs2[0:13]: base_sp_addr (14位)
    let base_sp_addr = (xs2 & 0x3FFF) as u32;
    
    // rs2[14:23]: iter (10位)
    let iter = ((xs2 >> 14) & 0x3FF) as u32;
    
    // rs2[24:33]: col_stride (10位)
    let col_stride = ((xs2 >> 24) & 0x3FF) as u32;
    
    Self {
      base_dram_addr,
      base_sp_addr,
      iter,
      col_stride,
    }
  }
}

/// Process MVIN instruction
pub fn process(xs1: u64, xs2: u64) -> u64 {
  let config = MvinConfig::from_fields(xs1, xs2);
  
  println!("  -> MVIN: dram_addr=0x{:08x}, sp_addr=0x{:04x}, iter={}, col_stride={}", 
    config.base_dram_addr, 
    config.base_sp_addr, 
    config.iter,
    config.col_stride
  );

  // DMA operation will be handled by simulator
  0
}
