/// Accelerator simulator with state management
use crate::builtin::Module;
use crate::config::NpuConfig;
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
  pub fn process(&mut self, funct: u32, xs1: u64, xs2: u64) -> std::io::Result<u64> {
    // 1. 将socket消息转换为指令并发送到顶层模块
    self.top.send_instruction(funct as u64, xs1, xs2);

    // 2. 运行一个时钟周期
    self.top.tick();

    // 3. 获取结果
    let result = self.top.get_mem_data() as u64;
    Ok(result)
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
