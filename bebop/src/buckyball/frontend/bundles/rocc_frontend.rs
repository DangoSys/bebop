use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoccInstruction {
    pub funct: u32,
    pub xs1: u64,
    pub xs2: u64,
}

impl RoccInstruction {
    pub fn new(funct: u32, xs1: u64, xs2: u64) -> Self {
        Self { funct, xs1, xs2 }
    }
}


