use super::tasks;
use crate::root::{chip::Chip, platform::Platform, shared_memory};
use crate::{
    bank::{BankConfig, BankMap},
    inst,
};
use std::{collections::BTreeMap, sync::{Arc, Mutex, Weak}};

pub struct Tile {
    pub(crate) tile_index: usize,
    pub(crate) tile_registry: Arc<Mutex<BTreeMap<usize, Weak<Tile>>>>,
    pub(crate) t2t: Mutex<super::t2t::Descriptor>,
    pub(crate) clint: Arc<bebop_clint::Clint>,
    pub(crate) exit_code: Arc<std::sync::atomic::AtomicI64>,
    pub(crate) harts: Vec<usize>,
    pub(crate) endpoint_ids: Vec<usize>,
    pub(crate) execution_ids: Vec<usize>,
    pub(crate) controller_id: Option<usize>,
    pub(crate) platform: Arc<Mutex<Platform>>,
    pub(crate) memory: Arc<crate::root::memory::Ddr>,
    pub(crate) banks: Mutex<shared_memory::State>,
    pub(crate) tasks: tasks::Tasks,
    pub(crate) private_endpoints: Mutex<BTreeMap<usize, Arc<Mutex<crate::accel::PrivateState>>>>,
}

impl Tile {
    /// Bank storage ownership is independent of the instruction issuer.
    pub(crate) fn bank_owner_id(&self, issuer: usize, shared: bool) -> usize {
        assert!(self.execution_ids.contains(&issuer), "bank trace issuer is outside its Tile");
        if shared { self.controller_id.unwrap_or(self.execution_ids[0]) } else { issuer }
    }

    pub fn new(
        chip: &Chip,
        topology: &crate::TileTopology,
        signatures: Vec<u64>,

    ) -> Arc<Self> {
        let harts = topology.cores.iter().filter(|(_,core)| crate::config::ant_config(*core).is_none()).map(|(_, core)| crate::config::core_hart_id(*core)).collect();
        let endpoint_ids: Vec<_> = topology.endpoint_cores.iter().map(|(_, core)| *core).collect();
        let compute_count = endpoint_ids.len();
        let mut map = BankMap::new(topology.shared_physical_bank_count);
        if topology.tile_index == 0 && endpoint_ids.is_empty() {
            for (bank, slot) in map.slots.iter_mut().enumerate() {
                slot.valid = true;
                slot.hart_id = topology.cores[0].1;
                slot.vbank_id = bank as u32;
            }
        }
        let tile = Arc::new(Self {
            tile_index: topology.tile_index,
            tile_registry: Arc::clone(&chip.tiles),
            t2t: Mutex::new(super::t2t::Descriptor::default()),
            clint: Arc::clone(&chip.clint),
            exit_code: Arc::clone(&chip.platform.lock().expect("BEMU platform poisoned").exit_code),
            harts,
            endpoint_ids,
            execution_ids: topology.cores.iter().map(|(_,core)| *core).collect(),
            controller_id: topology.controller_core,
            platform: Arc::clone(&chip.platform),
            memory: Arc::clone(&chip.memory),
            tasks: tasks::Tasks::new(&topology.worker_cores, signatures),
            private_endpoints: Mutex::new(BTreeMap::new()),
            banks: Mutex::new(shared_memory::State {
                storage: (0..topology.shared_physical_bank_count)
                    .map(|_| inst::instruction::PrivateBank::new(topology.shared_bank_size, false))
                    .collect(),
                cfgs: vec![BankConfig::default(); compute_count * topology.virtual_bank_count],
                map,
                virtual_bank_count: topology.virtual_bank_count,
            }),
        });
        let previous = chip.tiles.lock().expect("tile registry poisoned")
            .insert(topology.tile_index, Arc::downgrade(&tile));
        assert!(previous.is_none_or(|tile| tile.upgrade().is_none()), "duplicate live chip tile");
        tile
    }
}
