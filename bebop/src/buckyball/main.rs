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
      Box::new(Decoder::new(
        String::from("instruction"),
        String::from("push_to_rob"),
      )),
    ),
    Model::new(
      String::from("rob"),
      Box::new(Rob::new(
        16,
        String::from("receive_inst_from_decoder"),
        String::from("dispatch_to_rs"),
        String::from("commit_from_tdma"),
      )),
    ),
    Model::new(
      String::from("rs"),
      Box::new(Rs::new(
        String::from("receive_inst_from_rob"),
        String::from("issue_to_vecball"),
        String::from("issue_to_tdma_mvin"),
        String::from("issue_to_tdma_mvout"),
      )),
    ),
    Model::new(
      String::from("vector_ball"),
      Box::new(VectorBall::new(
        String::from("receive_inst_from_rs"),
        String::from("cmd_response_to_rs"),
        5.0,
      )),
    ),
    Model::new(
      String::from("bank"),
      Box::new(Bank::new(
        String::from("read_bank_req"),
        String::from("write_bank_req"),
        String::from("read_bank_resp"),
        String::from("write_bank_resp"),
        1.0,
        32,
        1024,
      )),
    ),
    Model::new(
      String::from("tdma"),
      Box::new(Tdma::new(
        String::from("mvin_req"),
        String::from("mvout_req"),
        String::from("read_bank_req"),
        String::from("write_bank_req"),
        String::from("read_bank_resp"),
        String::from("write_bank_resp"),
        String::from("commit_to_rob"),
      )),
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
      String::from("decoder"),                   // source_id: 从decoder发送
      String::from("rob"),                       // target_id: 发送到rob
      String::from("push_to_rob"),               // source_port: decoder的输出端口
      String::from("receive_inst_from_decoder"), // target_port: rob的输入端口
    ),
    Connector::new(
      String::from("rob_rs"),
      String::from("rob"),                   // source_id: 从rob发送
      String::from("rs"),                    // target_id: 发送到rs
      String::from("dispatch_to_rs"),        // source_port: rob的输出端口
      String::from("receive_inst_from_rob"), // target_port: rs的输入端口
    ),
    Connector::new(
      String::from("rs_vball"),
      String::from("rs"),                   // source_id: 从rs发送
      String::from("vector_ball"),          // target_id: 发送到vball
      String::from("issue_to_vecball"),     // source_port: rs的输出端口
      String::from("receive_inst_from_rs"), // target_port: vector_ball的输入端口
    ),
    Connector::new(
      String::from("rs_tdma_mvin"),
      String::from("rs"),                 // source_id: 从rs发送
      String::from("tdma"),               // target_id: 发送到tdma
      String::from("issue_to_tdma_mvin"), // source_port: rs的输出端口
      String::from("mvin_req"),           // target_port: tdma的输入端口
    ),
    Connector::new(
      String::from("rs_tdma_mvout"),
      String::from("rs"),                  // source_id: 从rs发送
      String::from("tdma"),                // target_id: 发送到tdma
      String::from("issue_to_tdma_mvout"), // source_port: rs的输出端口
      String::from("mvout_req"),           // target_port: tdma的输入端口
    ),
    Connector::new(
      String::from("tdma_bank_read_req"),
      String::from("tdma"),          // source_id: 从tdma发送
      String::from("bank"),          // target_id: 发送到bank
      String::from("read_bank_req"), // source_port: tdma的输出端口
      String::from("read_bank_req"), // target_port: bank的输入端口
    ),
    Connector::new(
      String::from("bank_tdma_read_resp"),
      String::from("bank"),           // source_id: 从bank发送
      String::from("tdma"),           // target_id: 发送到tdma
      String::from("read_bank_resp"), // source_port: bank的输出端口
      String::from("read_bank_resp"), // target_port: tdma的输入端口
    ),
    Connector::new(
      String::from("tdma_bank_write_req"),
      String::from("tdma"),           // source_id: 从tdma发送
      String::from("bank"),           // target_id: 发送到bank
      String::from("write_bank_req"), // source_port: tdma的输出端口
      String::from("write_bank_req"), // target_port: bank的输入端口
    ),
    Connector::new(
      String::from("bank_tdma_write_resp"),
      String::from("bank"),            // source_id: 从bank发送
      String::from("tdma"),            // target_id: 发送到tdma
      String::from("write_bank_resp"), // source_port: bank的输出端口
      String::from("write_bank_resp"), // target_port: tdma的输入端口
    ),
    Connector::new(
      String::from("tdma_rob_commit"),
      String::from("tdma"),            // source_id: 从tdma发送
      String::from("rob"),             // target_id: 发送到rob
      String::from("commit_to_rob"),   // source_port: tdma的输出端口
      String::from("commit_from_tdma"), // target_port: rob的输入端口
    ),
  ];

  Simulation::post(models, connectors)
}
