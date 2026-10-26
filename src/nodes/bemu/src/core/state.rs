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
    sync::{Arc, Mutex, Weak},
};

pub struct CoreState {
    pub(crate) hart_id: usize,
    pub(crate) core_registry: Arc<Mutex<BTreeMap<usize, Weak<CoreState>>>>,
    pub(crate) c2c: Mutex<crate::core::remote::Descriptor>,
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
            map: BankMap::new(params.shared_physical_bank_count),
            virtual_bank_count: params.virtual_bank_count,
        };
        let core = Arc::new(Self {
            hart_id: params.hart_id,
            core_registry: Arc::clone(&design.cores),
            c2c: Mutex::new(super::remote::Descriptor::default()),
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
