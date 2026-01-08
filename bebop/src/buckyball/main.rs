use sim::models::Model;
use sim::simulator::{Connector, Simulation};
use sim::utils::errors::SimulationError;

use super::bank::Bank;
use super::decoder::Decoder;
use super::rob::Rob;
use super::rs::Rs;
use super::tdma::Tdma;
use super::vector_ball::VectorBall;

pub fn create_simulation() -> Simulation {
  let models = vec![
    Model::new(
      String::from("decoder"),
      Box::new(Decoder::new(String::from("instruction"), String::from("decoded"))),
    ),
    Model::new(
      String::from("rob"),
      Box::new(Rob::new(16, String::from("decoded"), String::from("to_rs"))),
    ),
    Model::new(
      String::from("rs"),
      Box::new(Rs::new(String::from("to_rs"), String::from("to_exec"))),
    ),
    Model::new(
      String::from("vball"),
      Box::new(VectorBall::new(
        String::from("to_exec"),
        String::from("vector_done"),
        5.0,
      )),
    ),
    Model::new(
      String::from("bank"),
      Box::new(Bank::new(String::from("mem_req"), String::from("mem_resp"), 3.0)),
    ),
    Model::new(
      String::from("tdma"),
      Box::new(Tdma::new(String::from("mem_req"), String::from("mem_resp"))),
    ),
  ];

  let connectors = vec![
    // Pipeline: decoder -> rob -> rs -> ball/dma
    // Connector::new 的五个参数：
    // 1. id: 连接器的唯一标识符
    // 2. source_id: 源模型ID（消息发送方）
    // 3. target_id: 目标模型ID（消息接收方）
    // 4. source_port: 源模型的输出端口名
    // 5. target_port: 目标模型的输入端口名
    Connector::new(
      String::from("decoder_rob"),
      String::from("decoder"), // source_id: 从decoder发送
      String::from("rob"),     // target_id: 发送到rob
      String::from("decoded"), // source_port: decoder的输出端口
      String::from("decoded"), // target_port: rob的输入端口
    ),
    Connector::new(
      String::from("rob_rs"),
      String::from("rob"),   // source_id: 从rob发送
      String::from("rs"),    // target_id: 发送到rs
      String::from("to_rs"), // source_port: rob的输出端口
      String::from("to_rs"), // target_port: rs的输入端口
    ),
    Connector::new(
      String::from("rs_vball"),
      String::from("rs"),      // source_id: 从rs发送
      String::from("vball"),   // target_id: 发送到vball
      String::from("to_exec"), // source_port: rs的输出端口
      String::from("to_exec"), // target_port: vball的输入端口
    ),
    Connector::new(
      String::from("tdma_bank"),
      String::from("tdma"),    // source_id: 从tdma发送
      String::from("bank"),    // target_id: 发送到bank
      String::from("mem_req"), // source_port: tdma的输出端口
      String::from("mem_req"), // target_port: bank的输入端口
    ),
    Connector::new(
      String::from("bank_tdma"),
      String::from("bank"),     // source_id: 从bank发送
      String::from("tdma"),     // target_id: 发送到tdma
      String::from("mem_resp"), // source_port: bank的输出端口
      String::from("mem_resp"), // target_port: tdma的输入端口
    ),
  ];

  Simulation::post(models, connectors)
}
