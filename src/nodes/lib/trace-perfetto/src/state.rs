use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct OpenRob {
    pub domain_id: u64,
    pub name: String,
}

#[derive(Debug, Default)]
pub struct State {
    pub events: Vec<Value>,
    pub open_rob: HashMap<u64, OpenRob>,
    pub open_ctr: HashMap<(u64, u64), String>,
}
