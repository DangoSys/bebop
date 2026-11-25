use crate::banks::{BankConfig, BankMap};

/// The bank pool of one accelerator endpoint.
pub struct PrivateState {
    pub banks: Vec<crate::banks::PrivateBank>,
    pub bank_cfgs: Vec<BankConfig>,
    pub bank_map: BankMap,
    pub row_bytes: usize,
    pub shared_vbank_base: usize,
    pub max_private_vbank: usize,
}

/// The bank storage a hart shares between the accelerator endpoints inside it.
pub struct State {
    pub storage: Vec<crate::banks::PrivateBank>,
    pub cfgs: Vec<BankConfig>,
    pub map: BankMap,
    pub virtual_bank_count: usize,
    pub bank_entries: usize,
    pub moves: Vec<(usize, usize)>,
}
