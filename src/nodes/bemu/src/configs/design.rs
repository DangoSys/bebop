use super::types::{AcceleratorParams, BallDomainConfig, CoreParams, MemConfig};
use prost::Message;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/buckyball.config.rs"));

pub mod parameters {
    include!(concat!(env!("OUT_DIR"), "/ball_parameters.rs"));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Endpoint {
    Core(usize),
    Thread(usize),
}

impl Endpoint {
    pub fn execution_id(self) -> usize {
        match self {
            Self::Core(id) => device()
                .cores
                .iter()
                .position(|core| core.hart_id as usize == id)
                .expect("unknown hart ID"),
            Self::Thread(index) => device().cores.len() + index,
        }
    }
}

static DESIGN: OnceLock<Design> = OnceLock::new();

/// The design.pb of the simulation; its entry loads it before the device starts.
pub fn load(design_pb: &[u8]) {
    let design = Design::decode(design_pb).unwrap_or_else(|e| panic!("decode design.pb: {e}"));
    DESIGN.set(design).unwrap_or_else(|_| panic!("design.pb loaded twice"));
}

fn design() -> &'static Design {
    DESIGN.get().expect("design.pb is not loaded")
}

fn device() -> &'static DeviceConfig {
    design().device.as_ref().expect("design device")
}

pub fn shared_bank_fabric() -> &'static SharedBankFabricConfig {
    device().shared_bank_fabric.as_ref().expect("design shared-bank fabric")
}

pub fn hart_ids() -> Vec<usize> {
    device().cores.iter().map(|core| core.hart_id as usize).collect()
}

/// The Core component a hart is an instance of; it selects the core model.
pub fn core_component(id: usize) -> &'static str {
    &core(id).component
}

fn core(id: usize) -> &'static CorePlacement {
    device()
        .cores
        .iter()
        .find(|core| core.hart_id as usize == id)
        .expect("unknown hart ID")
}

fn accelerator(
    mem: &MemDomainConfig,
    frontend: &FrontendConfig,
    gp: &GpDomainConfig,
    domain: &BallDomain,
    banks: usize,
) -> AcceleratorParams {
    let bank = mem.bank.as_ref().expect("accelerator bank parameters");
    let mmio = mem.mmio.as_ref().expect("accelerator MMIO bank parameters");
    AcceleratorParams {
        virtual_bank_count: banks,
        gpdomain: gp.enable.expect("explicit GP enable").then(|| gp.clone()),
        mem_config: MemConfig {
            bank_num: bank.num as usize,
            bank_width: bank.width as usize,
            bank_entries: bank.entries as usize,
            mmio_enable: mmio.enable,
            mmio_bank_num: mmio.bank_num as usize,
            mmio_bank_entries: mmio.bank_entries as usize,
            mmio_bank_width: mmio.bank_width as usize,
            mmio_read_width: mmio.read_width as usize,
            private_vbank_upper_bound: frontend.vbank_id_upper_bound as usize,
            shared_vbank_base: frontend.shared_bank_id_base as usize,
        },
        ball_domain: BallDomainConfig {
            balls: domain.balls.clone(),
        },
    }
}

pub fn accelerator_for(endpoint: Endpoint) -> Option<&'static AcceleratorParams> {
    static CONFIGS: OnceLock<std::collections::BTreeMap<Endpoint, AcceleratorParams>> = OnceLock::new();
    CONFIGS
        .get_or_init(|| {
            let mut configs = std::collections::BTreeMap::new();
            let mut threads = std::collections::BTreeSet::new();
            for core in &device().cores {
                let banks = core.virtual_bank_count as usize;
                if let Some(acc) = &core.accelerator {
                    configs.insert(
                        Endpoint::Core(core.hart_id as usize),
                        accelerator(
                            acc.mem.as_ref().expect("local accelerator memory"),
                            acc.frontend.as_ref().expect("local accelerator frontend"),
                            acc.gpdomain.as_ref().expect("local accelerator GP"),
                            acc.balldomain.as_ref().expect("local accelerator BallDomain"),
                            banks,
                        ),
                    );
                }
                for &index in &core.thread_indices {
                    assert!(threads.insert(index), "thread attached to multiple cores");
                    let thread = &design().threads[index as usize];
                    assert_eq!(thread.index, index);
                    configs.insert(
                        Endpoint::Thread(index as usize),
                        accelerator(
                            thread.mem.as_ref().expect("thread memory"),
                            thread.frontend.as_ref().expect("thread frontend"),
                            thread.gpdomain.as_ref().expect("thread GP"),
                            thread.balldomain.as_ref().expect("thread BallDomain"),
                            banks,
                        ),
                    );
                }
            }
            configs
        })
        .get(&endpoint)
}

pub fn core_parameters(id: usize) -> CoreParams {
    let core = core(id);
    let shared = core.shared_mem.as_ref().expect("core shared memory parameters");
    let count = if shared.enable {
        assert!(shared.bank_entries > 0 && shared.entries > 0 && shared.entries % shared.bank_entries == 0);
        (shared.entries / shared.bank_entries) as usize
    } else {
        0
    };
    let mut endpoints = Vec::new();
    if core.accelerator.is_some() {
        endpoints.push(Endpoint::Core(id));
    }
    let threads = core
        .thread_indices
        .iter()
        .map(|&index| {
            let thread = &design().threads[index as usize];
            assert!(thread.local_id.is_some(), "thread has no local ID");
            assert!(
                ant_config(index as usize).is_some(),
                "this task controller requires Ant workers"
            );
            endpoints.push(Endpoint::Thread(index as usize));
            index as usize
        })
        .collect();
    CoreParams {
        hart_id: core.hart_id as usize,
        endpoints,
        threads,
        virtual_bank_count: core.virtual_bank_count as usize,
        shared_physical_bank_count: count,
        shared_bank_size: shared.bank_entries as usize * (shared.bank_width as usize / 8),
    }
}

pub fn ant_config(index: usize) -> Option<&'static AntConfig> {
    let thread = &design().threads[index];
    let cpu = thread.cpu.as_ref().expect("thread CPU parameters");
    match cpu.cpu.as_ref().expect("thread implementation") {
        cpu_config::Cpu::Ant(ant) => Some(ant),
    }
}
pub fn tss_config(index: usize) -> &'static SpmConfig {
    device()
        .cores
        .iter()
        .find(|core| core.thread_indices.contains(&(index as u32)))
        .expect("thread has no owning core")
        .tss
        .as_ref()
        .expect("thread requires TSS")
}
pub fn thread_local_id(index: usize) -> usize {
    design().threads[index].local_id.expect("local thread ID") as usize
}
pub fn thread_is_ant(index: usize) -> bool {
    ant_config(index).is_some()
}
pub fn thread_signature(index: usize) -> u64 {
    design().threads[index].signature.expect("thread signature")
}
/// The DDR address map: base, size, and the size of the model region at its top.
pub fn ddr() -> (u64, u64, u64) {
    let ddr = device().ddr.as_ref().expect("design DDR");
    (ddr.base, ddr.bytes, ddr.model)
}
pub fn core_count() -> usize {
    device().cores.len()
}
pub fn hart_capacity() -> usize {
    let harts: std::collections::BTreeSet<_> = device().cores.iter().map(|core| core.hart_id as usize).collect();
    assert_eq!(harts.len(), device().cores.len(), "duplicate hart IDs");
    harts.last().expect("design has no CPU harts") + 1
}
pub fn workload_placement(profile: &str) -> Result<usize, String> {
    let matches: Vec<_> = device().cores.iter().filter(|core| core.profile == profile).collect();
    match matches.as_slice() {
        [core] => Ok(core.hart_id as usize),
        [] => Err(format!(
            "profile {profile} has no directly attached core accelerator; use the design's task engine"
        )),
        _ => Err(format!("profile {profile} does not identify a unique core placement")),
    }
}
