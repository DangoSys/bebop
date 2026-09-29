use crate::{
    bank::{BankConfig, BankMap},
    inst,
};

pub(crate) struct State {
    pub(crate) storage: Vec<inst::instruction::PrivateBank>,
    pub(crate) cfgs: Vec<BankConfig>,
    pub(crate) map: BankMap,
    pub(crate) virtual_bank_count: usize,
}
