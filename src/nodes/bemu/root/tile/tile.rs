use super::tasks;
use crate::root::{chip::Chip, platform::Platform, shared_memory};
use crate::{
    bank::{BankConfig, BankMap},
    inst,
};
use std::{collections::BTreeMap, sync::{Arc, Mutex}};

pub struct Tile {
    pub(crate) clint: Arc<bebop_clint::Clint>,
    pub(crate) exit_code: Arc<std::sync::atomic::AtomicI64>,
    pub(crate) harts: Vec<usize>,
    pub(crate) controller_hart: Option<usize>,
    pub(crate) endpoint_harts: Vec<usize>,
    pub(crate) worker_harts: Vec<usize>,
    pub(crate) platform: Arc<Mutex<Platform>>,
    pub(crate) memory: Arc<crate::root::memory::Ddr>,
    pub(crate) banks: Mutex<shared_memory::State>,
    pub(crate) tasks: tasks::Tasks,
    pub(crate) has_scheduler: bool,
    pub(crate) private_endpoints: Mutex<BTreeMap<usize, Arc<Mutex<crate::accel::PrivateState>>>>,
}

impl Tile {
    /// Bank storage ownership is independent of the instruction issuer.
    pub(crate) fn bank_owner_hart(&self, issuer: usize, shared: bool) -> usize {
        assert!(self.harts.contains(&issuer), "bank trace issuer is outside its Tile");
        if shared { self.controller_hart.unwrap_or(self.harts[0]) } else { issuer }
    }

    pub fn new(
        chip: &Chip,
        topology: &crate::TileTopology,
        signatures: Vec<u64>,

    ) -> Arc<Self> {
        let harts = topology.cores.iter().map(|(_, core)| crate::config::core_hart_id(*core)).collect();
        let controller_hart = topology.controller_core.map(crate::config::core_hart_id);
        let endpoint_harts: Vec<_> = topology.endpoint_cores.iter().map(|(_, core)| crate::config::core_hart_id(*core)).collect();
        let worker_harts: Vec<_> = topology.worker_cores.iter().map(|(_, core)| crate::config::core_hart_id(*core)).collect();
        assert!(signatures.is_empty() || (controller_hart.is_some() && signatures.len() == worker_harts.len()), "task signatures require the explicit controller worker table");
        let compute_count = endpoint_harts.len();
        Arc::new(Self {
            clint: Arc::clone(&chip.clint),
            exit_code: Arc::clone(&chip.platform.lock().expect("BEMU platform poisoned").exit_code),
            harts,
            controller_hart,
            endpoint_harts,
            worker_harts,
            platform: Arc::clone(&chip.platform),
            memory: Arc::clone(&chip.memory),
            has_scheduler: controller_hart.is_some() && !signatures.is_empty(),
            tasks: tasks::Tasks::new(signatures),
            private_endpoints: Mutex::new(BTreeMap::new()),
            banks: Mutex::new(shared_memory::State {
                storage: (0..topology.shared_physical_bank_count)
                    .map(|_| inst::instruction::PrivateBank::new(topology.shared_bank_size, false))
                    .collect(),
                cfgs: vec![BankConfig::default(); compute_count * topology.virtual_bank_count],
                map: BankMap::new(topology.shared_physical_bank_count),
                virtual_bank_count: topology.virtual_bank_count,
            }),
        })
    }
}
