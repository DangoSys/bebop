use crate::buckyball::lib::msg::inject_latency;
use sim::simulator::Simulation;

/// ROB 条目 ID

/// 分配 ROB 条目
///
/// # 参数
/// - `cycle_sim`: 周期仿真实例
/// - `decoded_inst`: 解码后的指令 (funct, xs1, xs2, domain_id)
///
/// # 返回
/// - ROB 条目 ID
pub fn rob_allocate(cycle_sim: &mut Simulation, decoded_inst: (u32, u64, u64, u8)) -> (u32, u32, u64, u64, u8) {
  let (funct, xs1, xs2, domain_id) = decoded_inst;

  inject_latency(cycle_sim, "rob", 0.5, None, None, None);

  static mut ROB_COUNTER: u32 = 0;
  unsafe {
    ROB_COUNTER += 1;
    (ROB_COUNTER, funct, xs1, xs2, domain_id)
  }
}
