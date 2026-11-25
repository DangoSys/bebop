use generic::shared::{self, PrivateState};
use generic::{
    banks::{BankConfig, BankMap, PrivateBank},
    guest::ThreadMemory,
    hlink::{Attachment, HLink},
    tasks::Tasks,
    timing::{CpuCache, HartTime},
    tlink::TLink,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};

/// A hart's state: the link it offers its threads and DMA masters, and the bank pools of its
/// accelerator endpoints.
pub struct HartState {
    pub link: Arc<TLink>,
    pub cache: Arc<CpuCache>,
    pub service: Option<Arc<dyn generic::hlink::Service>>,
    pub exit_code: Arc<std::sync::atomic::AtomicI64>,
    pub endpoint_ids: Vec<usize>,
    pub execution_ids: Vec<usize>,
    pub pages: Arc<Mutex<generic::ddr::Pages>>,
    pub banks: Arc<Mutex<shared::State>>,
    pub shared_owner: usize,
    pub private_endpoints: Arc<Mutex<BTreeMap<usize, Arc<Mutex<PrivateState>>>>>,
}

impl HartState {
    pub fn install_thread_memory(&self, memory: Arc<dyn ThreadMemory>) {
        assert!(!self.link.tasks.started.load(std::sync::atomic::Ordering::Acquire));
        assert!(
            self.link.thread_memory.set(memory).is_ok(),
            "Thread memory is already connected"
        );
    }
    /// A hart implementation connects its bank pools and local transfer endpoints.
    pub fn connect_banks(&self, allocation: Vec<std::ops::Range<usize>>, moves: Vec<(usize, usize)>) {
        assert!(!self.link.tasks.started.load(std::sync::atomic::Ordering::Acquire));
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

    pub fn new(design: &HLink, params: &generic::configs::CoreParams, dma: generic::timing::DmaLayout) -> Arc<Self> {
        assert_eq!(
            dma.channel_by_source.len(),
            params.threads.len() + 1,
            "DMA source wiring differs from the hart endpoints"
        );
        let owner = generic::configs::Endpoint::Hart(params.hart_id).execution_id();
        let endpoints: Vec<_> = params
            .endpoints
            .iter()
            .map(|endpoint| endpoint.execution_id())
            .collect();
        let mut execution_ids: Vec<_> = params
            .threads
            .iter()
            .map(|&index| generic::configs::Endpoint::Thread(index).execution_id())
            .collect();
        execution_ids.push(owner);
        let banks = shared::State {
            bank_entries: params.shared_bank_entries,
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
        let hart = Arc::new(Self {
            link: Arc::new(TLink {
                hart_id: params.hart_id,
                clint: Arc::clone(&design.clint),
                platform: Arc::clone(&design.peripherals),
                memory: Arc::clone(&design.memory),
                transfers: Mutex::new(HartTime::new(Arc::clone(&design.timing), dma)),
                thread_memory: OnceLock::new(),
                tasks: Tasks::new(&params.threads, Arc::clone(&design.clint)),
            }),
            cache: Arc::new(CpuCache::new(params.data_cache_lines)),
            service: design.service.lock().expect("Device service poisoned").clone(),
            exit_code: Arc::clone(&design.exit_code),
            endpoint_ids: endpoints,
            execution_ids,
            pages: Arc::clone(&design.pages),
            private_endpoints: Arc::new(Mutex::new(BTreeMap::new())),
            banks: Arc::new(Mutex::new(banks)),
            shared_owner: owner,
        });
        design.attach(Attachment {
            hart_id: params.hart_id,
            cache: Arc::clone(&hart.cache),
            threads: hart
                .link
                .tasks
                .contexts
                .iter()
                .map(|context| Arc::clone(&context.progress))
                .collect(),
        });
        hart
    }
}

/// The shared banks as the accelerator on the RoCC port reaches them.
impl HartState {
    pub fn endpoint_index(&self, endpoint: generic::configs::Endpoint) -> usize {
        let id = endpoint.execution_id();
        self.endpoint_ids
            .iter()
            .position(|candidate| *candidate == id)
            .expect("accelerator is not connected to this Hart")
    }

    pub fn bank_owner_id(&self, issuer: usize, shared: bool) -> usize {
        assert!(
            self.execution_ids.contains(&issuer),
            "bank trace issuer is outside its HartState"
        );
        if shared {
            self.shared_owner
        } else {
            issuer
        }
    }

    pub fn connect(&self, endpoint: usize, private: Arc<Mutex<PrivateState>>) {
        let previous = self
            .private_endpoints
            .lock()
            .expect("private endpoints poisoned")
            .insert(endpoint, private);
        assert!(previous.is_none(), "duplicate thread bank endpoint");
    }
}
