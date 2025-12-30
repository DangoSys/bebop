use serde::{Deserialize, Serialize};
use sim::models::ModelMessage;
use sim::utils::errors::SimulationError;

/// 通用的消息接收函数
/// 
/// 检查端口名是否匹配，如果匹配则反序列化消息内容
/// 
/// # 示例
/// ```ignore
/// if let Ok(inst) = receive_message::<RoccInstruction>(msg, &self.decode_inst) {
///   // 处理指令
/// }
/// ```
pub fn receive_message<T: for<'de> Deserialize<'de>>(
    msg: &ModelMessage,
    expected_port: &str,
) -> Result<T, SimulationError> {
    if msg.port_name != expected_port {
        return Err(SimulationError::InvalidMessage);
    }
    let data: T = serde_json::from_str(&msg.content)?;
    Ok(data)
}

/// 通用的消息创建函数
/// 
/// 将数据序列化并创建 ModelMessage
/// 
/// # 示例
/// ```ignore
/// let msg = create_message(&decoded_inst, &self.enter_rob)?;
/// msg_output.push(msg);
/// ```
pub fn create_message<T: Serialize>(
    data: &T,
    target_port: &str,
) -> Result<ModelMessage, SimulationError> {
    Ok(ModelMessage {
        port_name: target_port.to_string(),
        content: serde_json::to_string(data)?,
    })
}

