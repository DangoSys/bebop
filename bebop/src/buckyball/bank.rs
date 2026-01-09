use serde::{Deserialize, Serialize};
use sim::models::model_trait::{DevsModel, Reportable, ReportableModel, SerializableModel};
use sim::models::{ModelMessage, ModelRecord};
use sim::simulator::Services;
use sim::utils::errors::SimulationError;
use std::f64::INFINITY;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SRAM {
  data: Vec<u128>,
}

impl SRAM {
  fn new(depth: u64) -> Self {
    Self {
      data: vec![0; depth as usize],
    }
  }

  fn read(&self, addr: u64) -> u128 {
    if addr < self.data.len() as u64 {
      self.data[addr as usize]
    } else {
      0
    }
  }

  fn write(&mut self, addr: u64, data: u128) {
    if addr < self.data.len() as u64 {
      self.data[addr as usize] = data;
    }
  }
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bank {
  depth: u64,
  num_banks: u64,
  banks: Vec<SRAM>,
  read_bank_req_port: String,
  write_bank_req_port: String,
  read_bank_resp_port: String,
  write_bank_resp_port: String,
  latency: f64,
  until_next_event: f64,
  records: Vec<ModelRecord>,
  read_buffer: Vec<(u64, u64)>,
  write_buffer: Vec<(u64, u64, u128)>,
}

impl Bank {
  pub fn new(
    read_bank_req_port: String,
    write_bank_req_port: String,
    read_bank_resp_port: String,
    write_bank_resp_port: String,
    latency: f64,
    num_banks: u64,
    depth: u64,
  ) -> Self {
    Self {
      depth,
      num_banks,
      banks: (0..num_banks).map(|_| SRAM::new(depth)).collect(),
      read_bank_req_port,
      write_bank_req_port,
      read_bank_resp_port,
      write_bank_resp_port,
      latency,
      until_next_event: INFINITY,
      records: Vec::new(),
      read_buffer: Vec::new(),
      write_buffer: Vec::new(),
    }
  }
}

impl DevsModel for Bank {
  fn events_ext(&mut self, incoming_message: &ModelMessage, services: &mut Services) -> Result<(), SimulationError> {
    if incoming_message.port_name == self.read_bank_req_port {
      let (vbank_id, bank_addr) = serde_json::from_str::<(u64, u64)>(&incoming_message.content)
        .map_err(|_| SimulationError::InvalidModelState)?;
      self.read_buffer.push((vbank_id, bank_addr));
      self.until_next_event = self.latency;

      self.records.push(ModelRecord {
        time: services.global_time(),
        action: "receive_read_req".to_string(),
        subject: incoming_message.content.clone(),
      });
    } 
    
    if incoming_message.port_name == self.write_bank_req_port {
      let (vbank_id, bank_addr, data_lo, data_hi) = serde_json::from_str::<(u64, u64, u64, u64)>(&incoming_message.content)
        .map_err(|_| SimulationError::InvalidModelState)?;
      let data = (data_hi as u128) << 64 | (data_lo as u128);
      self.write_buffer.push((vbank_id, bank_addr, data));
      self.until_next_event = self.latency;

      self.records.push(ModelRecord {
        time: services.global_time(),
        action: "receive_write_req".to_string(),
        subject: incoming_message.content.clone(),
      });
    }

    Ok(())
  }

  fn events_int(&mut self, services: &mut Services) -> Result<Vec<ModelMessage>, SimulationError> {
    let mut messages = Vec::new();

    // Process read requests
    while !self.read_buffer.is_empty() {
      let req = self.read_buffer.remove(0);
      if req.0 < self.banks.len() as u64 {
        let data = self.banks[req.0 as usize].read(req.1);
        
        messages.push(ModelMessage {
          content: serde_json::to_string(&vec![data])
            .map_err(|_| SimulationError::InvalidModelState)?,
          port_name: self.read_bank_resp_port.clone(),
        });

        self.records.push(ModelRecord {
          time: services.global_time(),
          action: "read_complete".to_string(),
          subject: serde_json::to_string(&vec![data])
            .unwrap_or_default(),
        });
      }
    }

    // Process all write requests
    while !self.write_buffer.is_empty() {
      let (vbank_id, bank_addr, data) = self.write_buffer.remove(0);
      if vbank_id < self.banks.len() as u64 {
        self.banks[vbank_id as usize].write(bank_addr, data);
        
        messages.push(ModelMessage {
          content: serde_json::to_string(&vec!["success"])
            .map_err(|_| SimulationError::InvalidModelState)?,
          port_name: self.write_bank_resp_port.clone(),
        });

        self.records.push(ModelRecord {
          time: services.global_time(),
          action: "write_complete".to_string(),
          subject: serde_json::to_string(&vec!["success"])
            .unwrap_or_default(),
        });
      }
    }

    self.until_next_event = INFINITY;
    Ok(messages)
  }

  fn time_advance(&mut self, time_delta: f64) {
    self.until_next_event -= time_delta;
  }

  fn until_next_event(&self) -> f64 {
    self.until_next_event
  }
}

impl Reportable for Bank {
  fn status(&self) -> String {
    "normal".to_string()
  }

  fn records(&self) -> &Vec<ModelRecord> {
    &self.records
  }
}

impl ReportableModel for Bank {}

impl SerializableModel for Bank {
  fn get_type(&self) -> &'static str {
    "Bank"
  }
}
