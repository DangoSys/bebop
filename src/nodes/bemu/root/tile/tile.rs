use super::tasks;
use crate::root::{chip::Chip, platform::Platform, shared_memory};
use crate::{
    bank::{BankConfig, BankMap},
    inst,
};
use std::sync::{Arc, Mutex};

pub struct Tile {
    pub(crate) first_hart: usize,
    pub(crate) platform: Arc<Mutex<Platform>>,
    pub(crate) banks: Mutex<shared_memory::State>,
    pub(crate) tasks: tasks::Tasks,
}

impl Tile {
    pub fn new(
        chip: &Chip,
        first_hart: usize,
        core_count: usize,
        signatures: Vec<u64>,
        shared_physical_bank_count: usize,
        shared_bank_size: usize,
        virtual_bank_count: usize,
    ) -> Arc<Self> {
        Arc::new(Self {
            first_hart,
            platform: Arc::clone(&chip.platform),
            tasks: tasks::Tasks::new(signatures),
            banks: Mutex::new(shared_memory::State {
                storage: (0..shared_physical_bank_count)
                    .map(|_| inst::instruction::PrivateBank::new(shared_bank_size, false))
                    .collect(),
                cfgs: vec![BankConfig::default(); core_count * virtual_bank_count],
                map: BankMap::new(shared_physical_bank_count),
                virtual_bank_count,
            }),
        })
    }
}
