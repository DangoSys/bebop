use crate::thread::banks::{BankConfig, BankMap};

pub(crate) struct State {
    pub(crate) storage: Vec<crate::thread::banks::PrivateBank>,
    pub(crate) cfgs: Vec<BankConfig>,
    pub(crate) map: BankMap,
    pub(crate) virtual_bank_count: usize,
}
