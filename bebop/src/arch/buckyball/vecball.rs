use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use super::mem_ctrl::request_read_bank_for_vecball;

pub static VECBALL_INST_CAN_ISSUE: AtomicBool = AtomicBool::new(true);

// Instruction data (set by receive_vecball_inst, cleared when processed)
struct VecballInstData {
  op1_bank_id: u64,
  op2_bank_id: u64,
  wr_bank_id: u64,
  iter: u64,
  rob_id: u64,
}

static VECBALL_INST_DATA: Mutex<Option<VecballInstData>> = Mutex::new(None);

static VECBALL_STATE: Mutex<VecBallState> = Mutex::new(VecBallState::Idle);

// VectorBall states for matrix multiplication pipeline
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum VecBallState {
  Idle,
  WaitOp1,       // Waiting for operand 1 from bank (all 16 elements)
  WaitOp2,       // Waiting for operand 2 from bank (all 16 elements)
  Computing,     // Performing matrix multiplication
  WaitWriteResp, // Waiting for write completion
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorBall {
  ball_mem_write_req_port: String,
  mem_ball_read_resp_port: String,
  commit_to_rob_port: String,

  until_next_event: f64,
  current_inst: Option<String>,
  records: Vec<ModelRecord>,

  // Instruction fields
  state: VecBallState,
  op1_bank_id: u64,
  op2_bank_id: u64,
  wr_bank_id: u64,
  iter: u64,
  mode: u64,
  rob_id: u64,

  // Computation state
  op1_data: Vec<u128>,    // Operand 1 data (16 elements)
  op2_data: Vec<u128>,    // Operand 2 data (16 elements)
  result_data: Vec<u128>, // Result data (16 elements)

  // Latency parameters
  read_latency: f64,
  compute_latency: f64,
  write_latency: f64,
}

impl VectorBall {
  pub fn new(commit_to_rob_port: String, ball_mem_write_req_port: String, mem_ball_read_resp_port: String) -> Self {
    VECBALL_INST_CAN_ISSUE.store(true, Ordering::Relaxed);
    *VECBALL_INST_DATA.lock().unwrap() = None;
    Self {
      ball_mem_write_req_port,
      mem_ball_read_resp_port,
      commit_to_rob_port,

      until_next_event: INFINITY,
      current_inst: None,
      records: Vec::new(),

      state: VecBallState::Idle,
      op1_bank_id: 0,
      op2_bank_id: 0,
      wr_bank_id: 0,
      iter: 0,
      mode: 0,
      rob_id: 0,

      op1_data: vec![0; 16],
      op2_data: vec![0; 16],
      result_data: vec![0; 16],

      read_latency: 16.0,    // 16 cycles to read 16 elements
      compute_latency: 16.0, // 16 cycles for 16x16 matmul
      write_latency: 16.0,   // 16 cycles to write 16 elements
    }
  }
}

impl DevsModel for VectorBall {
  fn events_ext(&mut self, incoming_message: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    // Receive read response from bank (batch of 16 elements)
    if incoming_message.port_name == self.mem_ball_read_resp_port {
      let data_vec: Vec<u128> =
        serde_json::from_str(&incoming_message.content).map_err(|_| SimulationError::InvalidModelState)?;

      if data_vec.len() != 16 {
        return Err(SimulationError::InvalidModelState);
      }

      match self.state {
        VecBallState::WaitOp1 => {
          // Received all 16 elements of operand 1
          self.op1_data = data_vec;

          self.records.push(ModelRecord {
            time: services.global_time(),
            action: "received_op1_batch".to_string(),
            subject: format!("16 elements from bank {}", self.op1_bank_id),
          });

          // Now request operand 2
          self.state = VecBallState::WaitOp2;
          *VECBALL_STATE.lock().unwrap() = VecBallState::WaitOp2;
          self.until_next_event = 1.0;
        },
        VecBallState::WaitOp2 => {
          // Received all 16 elements of operand 2
          self.op2_data = data_vec;

          self.records.push(ModelRecord {
            time: services.global_time(),
            action: "received_op2_batch".to_string(),
            subject: format!("16 elements from bank {}", self.op2_bank_id),
          });

          // Start computing
          self.state = VecBallState::Computing;
          *VECBALL_STATE.lock().unwrap() = VecBallState::Computing;
          self.until_next_event = self.compute_latency;
        },
        _ => {},
      }

      return Ok(());
    }

    // Write response is single cycle (handled in events_int)
    Ok(())
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut messages = Vec::new();

    match self.state {
      VecBallState::Idle => {
        // Check for new instruction
        if let Some(inst) = VECBALL_INST_DATA.lock().unwrap().take() {
          self.op1_bank_id = inst.op1_bank_id;
          self.op2_bank_id = inst.op2_bank_id;
          self.wr_bank_id = inst.wr_bank_id;
          self.iter = inst.iter;
          self.mode = 0;
          self.rob_id = inst.rob_id;

          // Start by requesting operand 1 (all 16 elements at once)
          self.state = VecBallState::WaitOp1;
          *VECBALL_STATE.lock().unwrap() = VecBallState::WaitOp1;
          self.until_next_event = 1.0;

          self.records.push(ModelRecord {
            time: services.global_time(),
            action: "receive_inst".to_string(),
            subject: format!(
              "op1_bank={}, op2_bank={}, wr_bank={}, iter={}, rob_id={}",
              inst.op1_bank_id, inst.op2_bank_id, inst.wr_bank_id, inst.iter, inst.rob_id
            ),
          });
        } else {
          self.until_next_event = INFINITY;
        }
      },
      VecBallState::WaitOp1 => {
        // Wait state: keep sending read request to mem_ctrl every cycle
        request_read_bank_for_vecball(self.op1_bank_id, 0u64, 16u64, self.rob_id);

        self.records.push(ModelRecord {
          time: services.global_time(),
          action: "req_read_op1_batch".to_string(),
          subject: format!("bank={}, addr=0, count=16", self.op1_bank_id),
        });

        // Wait state: until_next_event should always be 1.0
        // This state waits for external event (read response)
        self.until_next_event = 1.0;
      },
      VecBallState::WaitOp2 => {
        // Wait state: keep sending read request to mem_ctrl every cycle
        request_read_bank_for_vecball(self.op2_bank_id, 0u64, 16u64, self.rob_id);

        self.records.push(ModelRecord {
          time: services.global_time(),
          action: "req_read_op2_batch".to_string(),
          subject: format!("bank={}, addr=0, count=16", self.op2_bank_id),
        });

        // Wait state: until_next_event should always be 1.0
        // This state waits for external event (read response)
        self.until_next_event = 1.0;
      },
      VecBallState::Computing => {
        // Perform matrix multiplication (simplified: element-wise multiply-accumulate)
        for i in 0..16 {
          let mut sum: u128 = 0;
          for j in 0..16 {
            let a = self.op1_data[j] as u64;
            let b = self.op2_data[j] as u64;
            sum = sum.wrapping_add((a.wrapping_mul(b)) as u128);
          }
          self.result_data[i] = sum;
        }

        self.records.push(ModelRecord {
          time: services.global_time(),
          action: "compute_done".to_string(),
          subject: format!("iter={}", self.iter),
        });

        // Send batch write request (bank_id, start_addr, data_vec)
        // Directly use u128 array for serialization
        let write_data = self.result_data.clone();

        let request = (self.rob_id, self.wr_bank_id, 0u64, write_data);
        messages.push(ModelMessage {
          content: serde_json::to_string(&request).map_err(|_| SimulationError::InvalidModelState)?,
          port_name: self.ball_mem_write_req_port.clone(),
        });

        self.records.push(ModelRecord {
          time: services.global_time(),
          action: "req_write_batch".to_string(),
          subject: format!("bank={}, addr=0, count=16", self.wr_bank_id),
        });

        // Move to wait for write response
        self.state = VecBallState::WaitWriteResp;
        *VECBALL_STATE.lock().unwrap() = VecBallState::WaitWriteResp;
        self.until_next_event = self.write_latency;
      },
      VecBallState::WaitWriteResp => {
        // Write response is single cycle, so check if write is complete
        if self.until_next_event <= 0.0 {
          // Write is done (response is single cycle, immediate completion)
          self.records.push(ModelRecord {
            time: services.global_time(),
            action: "write_batch_complete".to_string(),
            subject: format!("16 elements to bank {}", self.wr_bank_id),
          });

          // All iterations done (data was packed in one JSON), commit to ROB
          messages.push(ModelMessage {
            content: serde_json::to_string(&self.rob_id).map_err(|_| SimulationError::InvalidModelState)?,
            port_name: self.commit_to_rob_port.clone(),
          });

          self.records.push(ModelRecord {
            time: services.global_time(),
            action: "commit".to_string(),
            subject: format!("rob_id={}", self.rob_id),
          });

          self.state = VecBallState::Idle;
          *VECBALL_STATE.lock().unwrap() = VecBallState::Idle;
          self.until_next_event = 1.0;
          VECBALL_INST_CAN_ISSUE.store(true, Ordering::Relaxed);
        }
      },
    }

    Ok(messages)
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    if self.state == VecBallState::Idle && VECBALL_INST_DATA.lock().unwrap().is_some() {
      return 0.0;
    }
    if self.state == VecBallState::WaitOp1 || self.state == VecBallState::WaitOp2 {
      return 1.0;
    }
    self.until_next_event
  }
}

impl Reportable for VectorBall {
  fn status(&self) -> String {
    format!("state={:?}, iter={}", self.state, self.iter)
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.records
  }
}

impl ReportableModel for VectorBall {}

impl SerializableModel for VectorBall {
  fn get_type(&self) -> &'static str {
    "VectorBall"
  }
}

/// Decode bb_mul_warp16 instruction
/// Returns: (op1_bank_id, op2_bank_id, wr_bank_id, iter)
fn decode_inst(xs1: u64, xs2: u64) -> (u64, u64, u64, u64) {
  // Decode fields from xs1
  let op1_bank_id = (xs1 & 0xFF) as u64; // bits[0:7]
  let op2_bank_id = ((xs1 >> 8) & 0xFF) as u64; // bits[8:15]

  // Decode fields from xs2
  let wr_bank_id = (xs2 & 0xFF) as u64; // bits[0:7]
  let iter = ((xs2 >> 8) & 0xFFFF) as u64; // bits[8:23]

  (op1_bank_id, op2_bank_id, wr_bank_id, iter)
}

/// Receive VectorBall instruction (called by RS or other modules)
/// Caller should check VECBALL_INST_CAN_ISSUE before calling this function
pub fn receive_vecball_inst(xs1: u64, xs2: u64, rob_id: u64) {
  // Decode instruction
  let (op1_bank_id, op2_bank_id, wr_bank_id, iter) = decode_inst(xs1, xs2);

  // Set instruction data
  *VECBALL_INST_DATA.lock().unwrap() = Some(VecballInstData {
    op1_bank_id,
    op2_bank_id,
    wr_bank_id,
    iter,
    rob_id,
  });

  // Mark as busy
  VECBALL_INST_CAN_ISSUE.store(false, Ordering::Relaxed);
}

pub fn is_vecball_idle() -> bool {
  *VECBALL_STATE.lock().unwrap() == VecBallState::Idle
}
