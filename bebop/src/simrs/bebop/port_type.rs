use serde::{Deserialize, Serialize};

/// Port 状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PortState {
  Idle,
  Busy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputPort {
  pub name: String,
  pub state: PortState,
}

impl InputPort {
  pub fn new(name: String) -> Self {
    Self {
      name,
      state: PortState::Idle,
    }
  }

  pub fn is_idle(&self) -> bool {
    self.state == PortState::Idle
  }

  pub fn set_busy(&mut self) {
    self.state = PortState::Busy;
  }

  pub fn set_idle(&mut self) {
    self.state = PortState::Idle;
  }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputPort {
  pub name: String,
}

impl OutputPort {
    pub fn new(name: String) -> Self {
      Self {
        name,
      }
    }
  }