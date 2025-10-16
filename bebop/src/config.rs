/// NPU配置参数

/// NPU配置
#[derive(Clone, Debug)]
pub struct NpuConfig {
  /// Scratchpad内存大小（u32个数）
  pub mem_size: usize,
  // 未来可以添加更多参数，例如：
  // pub num_pe: usize,           // PE数量
  // pub cache_size: usize,        // Cache大小
  // pub max_batch_size: usize,    // 最大batch size
  // pub clock_freq_mhz: u32,      // 时钟频率
}

impl NpuConfig {
  /// 创建默认配置
  pub fn new() -> Self {
    Self {
      mem_size: 1024, // 默认1024个u32 = 4KB
    }
  }

  /// 自定义配置
  pub fn with_mem_size(mem_size: usize) -> Self {
    Self { mem_size }
  }
}

impl Default for NpuConfig {
  fn default() -> Self {
    Self::new()
  }
}
