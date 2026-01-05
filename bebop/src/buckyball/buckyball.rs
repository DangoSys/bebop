use std::any::Any;
use std::io;

use serde::de;
use sim::models::{Model, DevsModel};
use sim::simulator::{Connector, Simulation};
use sim::utils::errors::SimulationError;

use crate::buckyball::cycleModel::CycleModel;
use crate::buckyball::frontend::{Decoder, Rob, Rs};
use crate::buckyball::frontend::{global_decode, rob_allocate, rs_dispatch};

pub struct Buckyball {
  cycle_sim: Simulation,
  all_models_busy: bool,
  decoded_inst: Option<(u32, u64, u64, u8)>,
  tmp1: Option<(u32, u64, u64, u8)>,
  rob_entry: Option<(u32, u32, u64, u64, u8)>,
  tmp2: Option<(u32, u32, u64, u64, u8)>,
  rs_entry: Option<(u32, u32, u64, u64, u8)>,
  tmp3: Option<(u32, u32, u64, u64, u8)>,
}

impl Buckyball {
  pub fn new() -> Self {
    let cycle_model = CycleModel::new();
    let decoder = Decoder::new();
    let rob = Rob::new();
    let rs = Rs::new();
    let decoded_inst = None;
    let tmp1 = None;
    let rob_entry = None;
    let tmp2 = None;
    let rs_entry = None;
    let tmp3 = None;

    let models = vec![
      Model::new("buckyball".to_string(), Box::new(cycle_model)),
      Model::new("decoder".to_string(), Box::new(decoder)),
      Model::new("rob".to_string(), Box::new(rob)),
      Model::new("rs".to_string(), Box::new(rs)),
    ];

    let connectors = vec![];

    let cycle_sim = Simulation::post(models, connectors);

    Self { cycle_sim, all_models_busy: false, decoded_inst, tmp1, rob_entry, tmp2, rs_entry, tmp3 }
  }


  pub fn model_next_event_latency(&mut self, model_id: &str) -> Option<f64> {
    self.cycle_sim
      .models()
      .iter()
      .find(|m| m.id() == model_id)
      .map(|m| DevsModel::until_next_event(*m))
  }

  pub fn model_idle(&mut self, model_id: &str) -> bool {
    self.model_next_event_latency(model_id)
      .unwrap_or(f64::INFINITY) == f64::INFINITY
  }

  pub fn model_ready(&mut self, model_id: &str) -> bool {
    self.model_next_event_latency(model_id)
      .unwrap_or(f64::INFINITY) == 1.0
  }

  pub fn model_update(&mut self, model_id: &str) -> bool {
    self.model_next_event_latency(model_id)
      .unwrap_or(f64::INFINITY) == 0.5
  }

  pub fn next_event_time(&mut self) -> f64 {
    if !self.cycle_sim.get_msg_output().is_empty() {
      return 0.0;
    }
    self.cycle_sim
      .models()
      .iter()
      .fold(f64::INFINITY, |min, model| {
        f64::min(min, model.until_next_event())
      })
  }



  pub fn inst_execute(&mut self, (funct, xs1, xs2): (Option<u32>, Option<u64>, Option<u64>)) {
    let mut raw_inst: (Option<u32>, Option<u64>, Option<u64>) = (None, None, None);
    if funct != None && xs1 != None && xs2 != None {
      raw_inst = (funct, xs1, xs2);
    }

    let decoder_ready = self.model_ready("decoder");
    if decoder_ready && raw_inst != (None, None, None) {
      self.tmp1 = Some(global_decode(&mut self.cycle_sim, raw_inst));
      raw_inst = (None, None, None);
    } 
    if self.model_update("decoder") {
      self.decoded_inst = self.tmp1;
      println!("decoded_inst: {:?}", self.decoded_inst);
      self.tmp1 = None;
    }
    

    let rob_ready = self.model_ready("rob");
    if rob_ready && self.decoded_inst != None {
      self.tmp2 = Some(rob_allocate(&mut self.cycle_sim, self.decoded_inst.unwrap()));
    } 
    if self.model_update("rob") {
      self.rob_entry = self.tmp2;
      println!("rob_entry: {:?}", self.rob_entry);
      self.tmp2 = None;
    }

    let rs_ready = self.model_ready("rs");
    if rs_ready && self.rob_entry != None {
      self.tmp3 = Some(rs_dispatch(&mut self.cycle_sim, self.rob_entry.unwrap()));
    } 
    if self.model_update("rs") {
      self.rs_entry = self.tmp3;
      println!("rs_entry: {:?}", self.rs_entry);
      self.tmp3 = None;
    }


  }

  pub fn cycle_advance(&mut self) -> io::Result<()> {
      let time1: f64 = self.cycle_sim.get_global_time();
      println!("before cycle advance");
    println!("decoder until next event: {:.1}", self.model_next_event_latency("decoder").unwrap_or(f64::INFINITY));
    println!("rob until next event: {:.1}", self.model_next_event_latency("rob").unwrap_or(f64::INFINITY));
    println!("rs until next event: {:.1}", self.model_next_event_latency("rs").unwrap_or(f64::INFINITY));
    // println!("time1: {:.1}", time1);
    // if !self.all_models_busy {
      if self.next_event_time() < 1.0 {
    loop {
      let time1: f64 = self.cycle_sim.get_global_time();
        let messages = self
          .cycle_sim
          .step()
          .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{:?}", e)))?;
        let time2: f64 = self.cycle_sim.get_global_time();
        if time2 > time1 {
          break;
        }
      }
    }
    println!("after cycle advance");
    println!("decoder until next event: {:.1}", self.model_next_event_latency("decoder").unwrap_or(f64::INFINITY));
    println!("rob until next event: {:.1}", self.model_next_event_latency("rob").unwrap_or(f64::INFINITY));
    println!("rs until next event: {:.1}", self.model_next_event_latency("rs").unwrap_or(f64::INFINITY));
    
    // let next_event = self.next_event_time();
    // if next_event == 1.0 {
    //   self.all_models_busy = true;
    //   println!("all models are busy, next event ({:.1})", next_event);
    //   return Ok(());
    // }
    
        let time2: f64 = self.cycle_sim.get_global_time();
        println!("Time: {:.1} -> {:.1}", time1, time2);
    Ok(())
  }
}
