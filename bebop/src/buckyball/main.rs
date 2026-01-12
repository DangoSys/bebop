use sim::models::Model;
use sim::simulator::{Connector, Simulation};

use super::bank::Bank;
use super::decoder::Decoder;
use super::mem_ctrl::MemController;
use super::rob::Rob;
use super::rs::Rs;
use super::tdma_loader::TdmaLoader;
use super::tdma_storer::TdmaStorer;
use super::vecball::VectorBall;

pub fn create_simulation() -> Simulation {
  let models = vec![
    Model::new(
      String::from("decoder"),
      Box::new(Decoder::new(String::from("instruction"), String::from("push_to_rob"))),
    ),
    Model::new(
      String::from("rob"),
      Box::new(Rob::new(
        16,
        String::from("receive_inst_from_decoder"),
        String::from("dispatch_to_rs"),
        String::from("commit"),
      )),
    ),
    Model::new(
      String::from("rs"),
      Box::new(Rs::new(String::from("receive_inst_from_rob"))),
    ),
    Model::new(
      String::from("vector_ball"),
      Box::new(VectorBall::new(
        String::from("commit_to_rob"),
        String::from("vball_mem_write_req"),
        String::from("mem_vball_read_resp"),
      )),
    ),
    Model::new(
      String::from("mem_controller"),
      Box::new(MemController::new(
        String::from("tdma_mem_write_req"),
        String::from("vball_mem_write_req"),
        String::from("mem_tdma_read_resp"),
        String::from("mem_vball_read_resp"),
        String::from("mem_bank_write_req"),
        String::from("bank_mem_read_resp"),
      )),
    ),
    Model::new(
      String::from("bank"),
      Box::new(Bank::new(
        String::from("mem_bank_write_req"),
        String::from("bank_mem_read_resp"),
        1.0,
        32,
        1024,
      )),
    ),
    Model::new(
      String::from("tdma_loader"),
      Box::new(TdmaLoader::new(
        String::from("tdma_mem_write_req"),
        String::from("commit_to_rob"),
      )),
    ),
    Model::new(
      String::from("tdma_storer"),
      Box::new(TdmaStorer::new(
        String::from("mem_tdma_read_resp"),
        String::from("commit_to_rob"),
      )),
    ),
  ];

  let connectors = vec![
    // Pipeline: decoder -> rob -> rs
    Connector::new(
      String::from("decoder_rob"),
      String::from("decoder"),
      String::from("rob"),
      String::from("push_to_rob"),
      String::from("receive_inst_from_decoder"),
    ),
    Connector::new(
      String::from("rob_rs"),
      String::from("rob"),
      String::from("rs"),
      String::from("dispatch_to_rs"),
      String::from("receive_inst_from_rob"),
    ),
    // TDMA Loader <-> MemController (write request is multi-cycle)
    Connector::new(
      String::from("tdma_loader_memctrl_write_req"),
      String::from("tdma_loader"),
      String::from("mem_controller"),
      String::from("tdma_mem_write_req"),
      String::from("tdma_mem_write_req"),
    ),
    // TDMA Storer <-> MemController (read response is multi-cycle)
    Connector::new(
      String::from("memctrl_tdma_storer_read_resp"),
      String::from("mem_controller"),
      String::from("tdma_storer"),
      String::from("mem_tdma_read_resp"),
      String::from("mem_tdma_read_resp"),
    ),
    // VectorBall <-> MemController (write request and read response are multi-cycle)
    Connector::new(
      String::from("vball_memctrl_write_req"),
      String::from("vector_ball"),
      String::from("mem_controller"),
      String::from("vball_mem_write_req"),
      String::from("vball_mem_write_req"),
    ),
    Connector::new(
      String::from("memctrl_vball_read_resp"),
      String::from("mem_controller"),
      String::from("vector_ball"),
      String::from("mem_vball_read_resp"),
      String::from("mem_vball_read_resp"),
    ),
    // MemController <-> Bank (write request and read response are multi-cycle)
    Connector::new(
      String::from("memctrl_bank_write_req"),
      String::from("mem_controller"),
      String::from("bank"),
      String::from("mem_bank_write_req"),
      String::from("mem_bank_write_req"),
    ),
    Connector::new(
      String::from("bank_memctrl_read_resp"),
      String::from("bank"),
      String::from("mem_controller"),
      String::from("bank_mem_read_resp"),
      String::from("bank_mem_read_resp"),
    ),
    // Commits to ROB
    Connector::new(
      String::from("tdma_loader_rob_commit"),
      String::from("tdma_loader"),
      String::from("rob"),
      String::from("commit_to_rob"),
      String::from("commit"),
    ),
    Connector::new(
      String::from("tdma_storer_rob_commit"),
      String::from("tdma_storer"),
      String::from("rob"),
      String::from("commit_to_rob"),
      String::from("commit"),
    ),
    Connector::new(
      String::from("vball_rob_commit"),
      String::from("vector_ball"),
      String::from("rob"),
      String::from("commit_to_rob"),
      String::from("commit"),
    ),
  ];

  Simulation::post(models, connectors)
}
