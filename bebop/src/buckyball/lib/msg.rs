use sim::simulator::{Message, Simulation};
use std::sync::atomic::Ordering;

/// 向指定模型发送延时消息
///
/// # 参数
/// - `simulation`: 仿真实例
/// - `target_model`: 目标模型名称
/// - `latency`: 延时（时间单位）
/// - `source_id`: 可选的消息来源ID，默认为 "default"
/// - `source_port`: 可选的来源端口，默认为 "default"
/// - `target_port`: 可选的目标端口，默认为 "default"
///
/// 如果周期模式未启用（CYCLE_MODE_ENABLED == false），此函数会直接返回
pub fn inject_latency(
  simulation: &mut Simulation,
  target_model: &str,
  latency: f64,
  source_id: Option<&str>,
  source_port: Option<&str>,
  target_port: Option<&str>,
) {
  // 检查全局标志，如果未启用则跳过
  if !crate::simulator::simulator::CYCLE_MODE_ENABLED.load(Ordering::Relaxed) {
    return;
  }

  let msg = Message::new(
    source_id.unwrap_or("default").to_string(),
    source_port.unwrap_or("default").to_string(),
    target_model.to_string(),
    target_port.unwrap_or("default").to_string(),
    simulation.get_global_time(),
    latency.to_string(),
  );
  simulation.inject_input(msg);
}
