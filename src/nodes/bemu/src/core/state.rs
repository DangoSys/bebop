use super::{shared, tasks};
use crate::{
    device::platform::Platform,
    thread::{
        banks::{BankConfig, BankMap, PrivateBank},
        execute::PrivateState,
    },
    DeviceModel,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};

pub struct CoreState {
    pub(crate) cache: Arc<crate::sim::timing::CpuCache>,
    pub(crate) transfers: Mutex<crate::sim::timing::CoreTime>,
    pub(crate) hart_id: usize,
    pub(crate) service: Option<Arc<dyn crate::device::Service>>,
    pub(crate) thread_memory: OnceLock<Arc<dyn super::memory::ThreadMemory>>,
    pub(crate) clint: Arc<crate::device::clint::Clint>,
    pub(crate) exit_code: Arc<std::sync::atomic::AtomicI64>,
    pub(crate) endpoint_ids: Vec<usize>,
    pub(crate) execution_ids: Vec<usize>,
    pub(crate) platform: Arc<Mutex<Platform>>,
    pub(crate) memory: Arc<crate::device::memory::Ddr>,
    pub(crate) banks: Arc<Mutex<shared::State>>,
    pub(crate) shared_owner: usize,
    pub(crate) tasks: tasks::Tasks,
    pub(crate) private_endpoints: Arc<Mutex<BTreeMap<usize, Arc<Mutex<PrivateState>>>>>,
}

impl CoreState {
    pub fn install_thread_memory(&self, memory: Arc<dyn super::memory::ThreadMemory>) {
        assert!(!self.tasks.started.load(std::sync::atomic::Ordering::Acquire));
        assert!(
            self.thread_memory.set(memory).is_ok(),
            "Thread memory is already connected"
        );
    }
    /// A core implementation connects its bank pools and local transfer endpoints.
    pub fn connect_banks(&self, allocation: Vec<std::ops::Range<usize>>, moves: Vec<(usize, usize)>) {
        assert!(!self.tasks.started.load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(allocation.len(), self.endpoint_ids.len());
        let mut banks = self.banks.lock().expect("shared banks poisoned");
        assert!(allocation
            .iter()
            .all(|range| range.start < range.end && range.end <= banks.storage.len()));
        assert!(moves
            .iter()
            .all(|&(source, target)| source < allocation.len() && target < allocation.len()));
        banks.map.allocation = allocation;
        banks.moves = moves;
    }

    pub fn endpoint_index(&self, endpoint: crate::Endpoint) -> usize {
        let id = endpoint.execution_id();
        self.endpoint_ids
            .iter()
            .position(|candidate| *candidate == id)
            .expect("accelerator is not connected to this Core")
    }

    /// Bank storage ownership is independent of the instruction issuer.
    pub(crate) fn bank_owner_id(&self, issuer: usize, shared: bool) -> usize {
        assert!(
            self.execution_ids.contains(&issuer),
            "bank trace issuer is outside its CoreState"
        );
        if shared {
            self.shared_owner
        } else {
            issuer
        }
    }

    pub fn new(design: &DeviceModel, params: &crate::CoreParams) -> Arc<Self> {
        let owner = crate::Endpoint::Core(params.hart_id).execution_id();
        let endpoints: Vec<_> = params
            .endpoints
            .iter()
            .map(|endpoint| endpoint.execution_id())
            .collect();
        let mut execution_ids: Vec<_> = params
            .threads
            .iter()
            .map(|&index| crate::Endpoint::Thread(index).execution_id())
            .collect();
        execution_ids.push(owner);
        let banks = shared::State {
            storage: (0..params.shared_physical_bank_count)
                .map(|_| PrivateBank::new(params.shared_bank_size, false))
                .collect(),
            cfgs: vec![BankConfig::default(); endpoints.len() * params.virtual_bank_count],
            map: BankMap {
                allocation: vec![0..params.shared_physical_bank_count; endpoints.len()],
                ..BankMap::new(params.shared_physical_bank_count)
            },
            virtual_bank_count: params.virtual_bank_count,
            moves: (0..endpoints.len())
                .flat_map(|source| (0..endpoints.len()).map(move |target| (source, target)))
                .collect(),
        };
        let core = Arc::new(Self {
            cache: Arc::new(crate::sim::timing::CpuCache::new(params.data_cache_lines)),
            transfers: Mutex::new(crate::sim::timing::CoreTime::new(
                Arc::clone(&design.timing),
                params.dma_cache_lines,
            )),
            hart_id: params.hart_id,
            service: design.service.lock().expect("Device service poisoned").clone(),
            thread_memory: OnceLock::new(),
            clint: Arc::clone(&design.clint),
            exit_code: Arc::clone(&design.platform.lock().expect("BEMU platform poisoned").exit_code),
            endpoint_ids: endpoints,
            execution_ids,
            platform: Arc::clone(&design.platform),
            memory: Arc::clone(&design.memory),
            tasks: tasks::Tasks::new(&params.threads),
            private_endpoints: Arc::new(Mutex::new(BTreeMap::new())),
            banks: Arc::new(Mutex::new(banks)),
            shared_owner: owner,
        });
        let previous = design
            .cores
            .lock()
            .expect("core registry poisoned")
            .insert(params.hart_id, Arc::downgrade(&core));
        assert!(
            previous.is_none_or(|core| core.upgrade().is_none()),
            "duplicate live design core"
        );
        design
            .instances
            .lock()
            .expect("device instances poisoned")
            .push(Arc::clone(&core));
        core
    }
}
