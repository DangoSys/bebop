/// Accelerator simulator with state management
use crate::builtin::Module;
use crate::config::NpuConfig;
use crate::memdomain::decoder::DmaOperation;
use crate::socket::DmaClient;
use crate::Top;

/// Accelerator simulator that manages state
pub struct Simulator {
  /// NPU顶层模块
  top: Top,
}

impl Simulator {
  pub fn new(config: NpuConfig) -> Self {
    Self {
      top: Top::new("npu_top", config.mem_size),
    }
  }

  /// Process an instruction from Spike
  /// 返回值表示指令执行结果
  pub fn process(&mut self, funct: u32, xs1: u64, xs2: u64, dma_client: &mut DmaClient) -> std::io::Result<u64> {
    // 1. 将socket消息转换为指令并发送到顶层模块
    self.top.send_instruction(funct as u64, xs1, xs2);

    // 2. 运行一个时钟周期
    self.top.tick();

    // 3. 检查是否有 DMA 操作需要执行
    if let Some(dma_op) = self.top.get_dma_operation() {
      self.execute_dma(&dma_op, dma_client)?;
    }

    // 4. 获取结果
    let result = self.top.get_mem_data() as u64;
    Ok(result)
  }

  /// 执行 DMA 操作
  fn execute_dma(&mut self, dma_op: &DmaOperation, dma_client: &mut DmaClient) -> std::io::Result<()> {
    match dma_op {
      DmaOperation::Mvin(config) => {
        println!("  [Simulator] Executing MVIN DMA operation");
        self.execute_mvin(config, dma_client)?;
      },
      DmaOperation::Mvout(config) => {
        println!("  [Simulator] Executing MVOUT DMA operation");
        self.execute_mvout(config, dma_client)?;
      },
    }
    Ok(())
  }

  /// 执行 MVIN DMA 操作 - 从 DRAM 读取到 scratchpad
  fn execute_mvin(
    &mut self,
    config: &crate::global_decoder::MvinConfig,
    dma_client: &mut DmaClient,
  ) -> std::io::Result<()> {
    const DIM: u32 = 16; // 每行的元素数量

    // 根据 bank 配置获取元素大小
    // 这里简化处理，假设每个元素是 1 字节 (elem_t = int8_t)
    let elem_size = 1u32; // 后续可以根据 bank 配置动态获取

    // 迭代执行 DMA 读取
    for i in 0..config.iter {
      for j in 0..DIM {
        // 计算 DRAM 地址
        let dram_addr = (config.base_dram_addr as u64)
          + ((i * DIM * elem_size) as u64)
          + ((j * elem_size) as u64);

        // 通过 DMA 读取数据
        let data = dma_client.dma_read(dram_addr, elem_size)?;

        // 计算 scratchpad 地址
        let spad_addr = config.base_sp_addr + i * DIM + j;

        // 写入 scratchpad
        self.top.write_spad(spad_addr as usize, data as u32);
      }
    }

    Ok(())
  }

  /// 执行 MVOUT DMA 操作 - 从 scratchpad 写入到 DRAM
  fn execute_mvout(
    &mut self,
    config: &crate::global_decoder::MvoutConfig,
    dma_client: &mut DmaClient,
  ) -> std::io::Result<()> {
    const DIM: u32 = 16; // 每行的元素数量

    // 根据 bank 配置获取元素大小
    let elem_size = 1u32; // 后续可以根据 bank 配置动态获取

    // 迭代执行 DMA 写入
    for i in 0..config.iter {
      for j in 0..DIM {
        // 计算 scratchpad 地址
        let spad_addr = config.base_sp_addr + i * DIM + j;

        // 从 scratchpad 读取数据
        let data = self.top.read_spad(spad_addr as usize) as u64;

        // 计算 DRAM 地址
        let dram_addr = (config.base_dram_addr as u64)
          + ((i * DIM * elem_size) as u64)
          + ((j * elem_size) as u64);

        // 通过 DMA 写入数据
        dma_client.dma_write(dram_addr, data, elem_size)?;
      }
    }

    Ok(())
  }

  /// 重置模拟器
  pub fn reset(&mut self) {
    self.top.reset();
  }

  /// 初始化内存数据
  pub fn init_mem(&mut self, addr: usize, data: u32) {
    self.top.init_mem(addr, data);
  }
}

impl Default for Simulator {
  fn default() -> Self {
    Self::new(NpuConfig::default())
  }
}
