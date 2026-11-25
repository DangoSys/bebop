use super::types::{AcceleratorParams, BallDomainConfig, CoreParams, MemConfig};
use prost::Message;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/buckyball.config.rs"));

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Endpoint {
    Hart(usize),
    Thread(usize),
}

impl Endpoint {
    pub fn execution_id(self) -> usize {
        match self {
            Self::Hart(id) => device()
                .harts
                .iter()
                .position(|hart| hart.hart_id as usize == id)
                .expect("unknown hart ID"),
            // A thread alone is the only execution of its simulation.
            Self::Thread(index) if alone().is_some() => index,
            Self::Thread(index) => device().harts.len() + index,
        }
    }
}

static DESIGN: OnceLock<Design> = OnceLock::new();

/// The design.pb of the simulation; its entry loads it before the device starts.
pub fn load(design_pb: &[u8]) {
    let design = Design::decode(design_pb).unwrap_or_else(|e| panic!("decode design.pb: {e}"));
    assert!(
        design.device.is_some() != design.thread.is_some(),
        "design.pb holds a device or a thread alone"
    );
    for hart in design.device.iter().flat_map(|device| &device.harts) {
        let memory = hart.memory.as_ref().expect("hart memory");
        assert!(
            memory.access.result_line_bits != 512 || memory.icache.line_result,
            "CPU fetch line response requires full-line instruction cache results"
        );
    }
    DESIGN.set(design).unwrap_or_else(|_| panic!("design.pb loaded twice"));
}

fn design() -> &'static Design {
    DESIGN.get().expect("design.pb is not loaded")
}

/// The host of a thread alone (`--thread`), whose design.pb has no device.
pub fn alone() -> Option<&'static ThreadHost> {
    design()
        .thread
        .as_ref()
        .map(|thread| thread.host.as_ref().expect("thread alone host"))
}

fn device() -> &'static DeviceConfig {
    design().device.as_ref().expect("design device")
}

pub fn trace() -> &'static TraceHardware {
    device().trace.as_ref().expect("design trace peripheral")
}

pub fn shared_bank_fabric() -> &'static SharedBankFabricConfig {
    device().shared_bank_fabric.as_ref().expect("design shared-bank fabric")
}

pub fn hart_ids() -> Vec<usize> {
    device().harts.iter().map(|hart| hart.hart_id as usize).collect()
}

/// The Hart component a hart is an instance of; it selects the hart model.
pub fn hart_component(id: usize) -> &'static str {
    &hart(id).component
}

fn hart(id: usize) -> &'static HartPlacement {
    device()
        .harts
        .iter()
        .find(|hart| hart.hart_id as usize == id)
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
        rob_entries: frontend.rob_entries as usize,
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
            max_private_vbank: frontend.max_private_vbank as usize,
            shared_vbank_base: frontend.shared_vbank_base as usize,
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
            if let Some(host) = alone() {
                let thread = &design().threads[0];
                let banks = host.virtual_bank_count as usize;
                let mut parameters = accelerator(
                    thread.mem.as_ref().expect("thread memory"),
                    thread.frontend.as_ref().expect("thread frontend"),
                    thread.gpdomain.as_ref().expect("thread GP"),
                    thread.balldomain.as_ref().expect("thread BallDomain"),
                    banks,
                );
                parameters.mem_config.shared_vbank_base = banks;
                configs.insert(Endpoint::Thread(0), parameters);
                return configs;
            }
            let mut threads = std::collections::BTreeSet::new();
            for hart in &device().harts {
                let banks = hart.virtual_bank_count as usize;
                if let Some(acc) = &hart.accelerator {
                    configs.insert(
                        Endpoint::Hart(hart.hart_id as usize),
                        accelerator(
                            acc.mem.as_ref().expect("local accelerator memory"),
                            acc.frontend.as_ref().expect("local accelerator frontend"),
                            acc.gpdomain.as_ref().expect("local accelerator GP"),
                            acc.balldomain.as_ref().expect("local accelerator BallDomain"),
                            banks,
                        ),
                    );
                }
                for &index in &hart.thread_indices {
                    assert!(threads.insert(index), "thread attached to multiple harts");
                    let thread = &design().threads[index as usize];
                    assert_eq!(thread.index, index);
                    let mut parameters = accelerator(
                        thread.mem.as_ref().expect("thread memory"),
                        thread.frontend.as_ref().expect("thread frontend"),
                        thread.gpdomain.as_ref().expect("thread GP"),
                        thread.balldomain.as_ref().expect("thread BallDomain"),
                        banks,
                    );
                    parameters.mem_config.shared_vbank_base = banks;
                    configs.insert(Endpoint::Thread(index as usize), parameters);
                }
            }
            configs
        })
        .get(&endpoint)
}

pub fn hart_parameters(id: usize) -> CoreParams {
    let hart = hart(id);
    let shared = hart.shared_mem.as_ref().expect("hart shared memory parameters");
    let count = if shared.enable {
        assert!(shared.bank_entries > 0 && shared.entries > 0 && shared.entries % shared.bank_entries == 0);
        (shared.entries / shared.bank_entries) as usize
    } else {
        0
    };
    let mut endpoints = Vec::new();
    if hart.accelerator.is_some() {
        endpoints.push(Endpoint::Hart(id));
    }
    let threads = hart
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
        data_cache_lines: hart.memory.as_ref().expect("hart memory").dcache.cache.lines as usize,
        pipeline: match hart.implementation.as_ref() {
            None => super::types::CpuPipeline::Sequential,
            Some(implementation) => match implementation.type_url.as_str() {
                "type.googleapis.com/buckyball.components.rocket.Params" => super::types::CpuPipeline::Rocket,
                // MAGIA's host CPU is the Rocket pipeline.
                "type.googleapis.com/buckyball.components.magia.Params" => super::types::CpuPipeline::Rocket,
                // Gemmini's host CPU is Chipyard's Rocket with L1 caches.
                "type.googleapis.com/buckyball.components.gemminihart.Params" => {
                    super::types::CpuPipeline::CachedRocket
                }
                // BOOM's native core keeps its L1 caches behind AXI; a first
                // version times it as the cached in-order Rocket, not as BOOM's
                // out-of-order pipeline.
                "type.googleapis.com/buckyball.components.boom.Params" => super::types::CpuPipeline::CachedRocket,
                // XiangShan likewise: its caches are native; a first version
                // times it as the cached in-order Rocket.
                "type.googleapis.com/buckyball.components.xiangshan.Params" => super::types::CpuPipeline::CachedRocket,
                other => panic!("unknown CPU timing implementation: {other}"),
            },
        },
        fetch_line: hart.memory.as_ref().expect("hart memory").access.result_line_bits == 512,
        hart_id: hart.hart_id as usize,
        endpoints,
        threads,
        virtual_bank_count: hart.virtual_bank_count as usize,
        shared_physical_bank_count: count,
        shared_bank_size: shared.bank_entries as usize * (shared.bank_width as usize / 8),
        shared_bank_entries: shared.bank_entries as usize,
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
    if let Some(host) = alone() {
        return host.tss.as_ref().expect("thread alone TSS");
    }
    device()
        .harts
        .iter()
        .find(|hart| hart.thread_indices.contains(&(index as u32)))
        .expect("thread has no owning hart")
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
/// The bank rows one beat of the DDR port carries.
pub fn ddr_beat_rows() -> u64 {
    device().ddr.as_ref().expect("design DDR").beat_rows as u64
}
pub fn hart_count() -> usize {
    device().harts.len()
}
pub fn hart_capacity() -> usize {
    let harts: std::collections::BTreeSet<_> = device().harts.iter().map(|hart| hart.hart_id as usize).collect();
    assert_eq!(harts.len(), device().harts.len(), "duplicate hart IDs");
    harts.last().expect("design has no CPU harts") + 1
}
pub fn workload_placement(profile: &str) -> Result<usize, String> {
    let matches: Vec<_> = device().harts.iter().filter(|hart| hart.profile == profile).collect();
    match matches.as_slice() {
        [hart] => Ok(hart.hart_id as usize),
        [] => Err(format!(
            "profile {profile} has no directly attached hart accelerator; use the design's task kernel"
        )),
        _ => Err(format!("profile {profile} does not identify a unique hart placement")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "CPU fetch line response requires full-line instruction cache results")]
    fn reject_fetch_line_without_icache_line_result() {
        let mut memory = MemoryConfig::default();
        memory.access.result_line_bits = 512;
        memory.icache.line_result = false;
        let hart = HartPlacement {
            memory: Some(memory),
            ..Default::default()
        };
        let device = DeviceConfig {
            harts: vec![hart],
            ..Default::default()
        };
        let design = Design {
            device: Some(device),
            ..Default::default()
        };
        load(&design.encode_to_vec());
    }
}
