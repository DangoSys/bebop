use prost::Message;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/buckyball.config.rs"));

const CHIP_PB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/chip.pb"));

#[derive(Clone)]
pub struct Topology {
    pub mem_config: MemConfig,
    pub ball_domain: BallDomainConfig,
    pub rvv: Option<RvvConfig>,
    pub virtual_bank_count: usize,
}

#[derive(Clone)]
pub struct MemConfig {
    pub bank_num: usize,
    pub bank_width: usize,
    pub bank_entries: usize,
    pub mmio_enable: bool,
    pub mmio_bank_num: usize,
    pub mmio_bank_entries: usize,
    pub mmio_bank_width: usize,
    pub mmio_read_width: usize,
    pub private_vbank_upper_bound: usize,
    pub shared_vbank_base: usize,
}

#[derive(Clone)]
pub struct BallDomainConfig {
    pub mappings: Vec<BallIdMapping>,
    pub isa: Vec<BallIsaEntry>,
}

pub struct TileTopology {
    pub tile_index: usize,
    /// BB-enabled bank endpoints, including the controller when it has BB.
    pub endpoint_cores: Vec<(String, usize)>,
    /// Execution cores excluding the explicit controller, including CPU-only cores.
    pub worker_cores: Vec<(String, usize)>,
    pub controller_core: Option<usize>,
    pub cores: Vec<(String, usize)>,
    pub has_buckyball: bool,
    pub virtual_bank_count: usize,
    pub shared_physical_bank_count: usize,
    pub shared_bank_size: usize,
}

fn chip() -> &'static Chip {
    static CHIP: OnceLock<Chip> = OnceLock::new();
    CHIP.get_or_init(|| Chip::decode(CHIP_PB).unwrap_or_else(|e| panic!("decode chip.pb: {e}")))
}

fn mem_of(core: &CoreInstance) -> &MemDomainConfig {
    core.mem
        .as_ref()
        .unwrap_or_else(|| panic!("core {} missing mem", core.index))
}

fn to_topology(core: &CoreInstance) -> Topology {
    let mem = mem_of(core);
    let bank = mem
        .bank
        .as_ref()
        .unwrap_or_else(|| panic!("core {} missing bank", core.index));
    let mmio = mem
        .mmio
        .as_ref()
        .unwrap_or_else(|| panic!("core {} missing mmio", core.index));
    let rvv = core.rvv.as_ref().and_then(|config| {
        config.enable.expect("rvv.enable must be explicitly configured")
            .then(|| config.clone())
    });
    Topology {
        virtual_bank_count: virtual_bank_count_for_core(core.index as usize),
        rvv,
        mem_config: MemConfig {
            bank_num: bank.num as usize,
            bank_width: bank.width as usize,
            bank_entries: bank.entries as usize,
            mmio_enable: mmio.enable,
            mmio_bank_num: mmio.bank_num as usize,
            mmio_bank_entries: mmio.bank_entries as usize,
            mmio_bank_width: mmio.bank_width as usize,
            mmio_read_width: mmio.read_width as usize,
            private_vbank_upper_bound: core
                .frontend
                .as_ref()
                .map_or(0, |frontend| frontend.vbank_id_upper_bound as usize),
            shared_vbank_base: core
                .frontend
                .as_ref()
                .map_or(0, |frontend| frontend.shared_bank_id_base as usize),
        },
        ball_domain: BallDomainConfig {
            mappings: core
                .balldomain
                .as_ref()
                .map_or_else(Vec::new, |ball| ball.mappings.iter().cloned().collect()),
            isa: core
                .balldomain
                .as_ref()
                .map_or_else(Vec::new, |ball| ball.isa.iter().cloned().collect()),
        },
    }
}

pub fn topology_for_core(core_index: usize) -> &'static Topology {
    static TOPOLOGIES: OnceLock<Vec<Topology>> = OnceLock::new();
    let topologies = TOPOLOGIES.get_or_init(|| chip().cores.iter().map(to_topology).collect());
    topologies
        .get(core_index)
        .unwrap_or_else(|| panic!("core index {core_index} out of range (n={})", topologies.len()))
}

pub fn virtual_bank_count_for_core(core_index: usize) -> usize {
    let c = chip();
    let mut count = None;
    for tile in &c.tiles {
        if tile.core_indices.iter().any(|&index| index as usize == core_index) {
            assert!(count.is_none(), "core {core_index} belongs to multiple tiles");
            assert!(
                tile.virtual_bank_count > 0,
                "core {core_index} tile has no virtual banks"
            );
            count = Some(tile.virtual_bank_count as usize);
        }
    }
    count.unwrap_or_else(|| panic!("core {core_index} belongs to no tile"))
}

pub fn tile_topology(tile_index: usize) -> TileTopology {
    let c = chip();
    let tile = c
        .tiles
        .get(tile_index)
        .unwrap_or_else(|| panic!("tile index {tile_index} out of range (n={})", c.tiles.len()));
    if tile.core_indices.is_empty() {
        panic!("tile {tile_index} has no cores");
    }
    let shared = tile
        .shared_mem
        .as_ref()
        .unwrap_or_else(|| panic!("tile {tile_index} missing shared_mem"));
    let compute_indices: Vec<_> = tile.core_indices.iter().copied().filter(|&index| {
        c.cores[index as usize].balldomain.as_ref()
            .is_some_and(|domain| domain.ball_num > 0)
    }).collect();
    assert!(compute_indices.is_empty() || tile.virtual_bank_count > 0, "NPU tile requires virtual banks");
    let bank_entries = shared.bank_entries as usize;
    assert!(bank_entries > 0, "tile shared bank_entries must be explicit");
    let bank_width = shared.bank_width as usize;
    assert_eq!(bank_width, 128, "shared bank width must be 128 bits");
    for &core_index in &compute_indices {
        let core = &c.cores[core_index as usize];
        let bank = mem_of(core)
            .bank
            .as_ref()
            .unwrap_or_else(|| panic!("core {} missing bank", core.index));
        assert_eq!(
            bank.width as usize, bank_width,
            "tile {tile_index} cores have different bank widths"
        );
    }
    let shared_physical_bank_count = if shared.enable {
        assert!(shared.entries > 0, "tile {tile_index} shared entries is 0");
        assert_eq!(
            shared.entries as usize % bank_entries,
            0,
            "tile {tile_index} shared entries must be divisible by bank entries"
        );
        shared.entries as usize / bank_entries
    } else {
        0
    };
    let cores = tile
        .core_indices
        .iter()
        .map(|&index| {
            let index = index as usize;
            let role = c.cores[index].role.clone();
            (role, index)
        })
        .collect();
    let endpoint_cores = compute_indices.into_iter().map(|index| {
        let index = index as usize;
        (c.cores[index].role.clone(), index)
    }).collect();
    let controller_core = tile.controller_core_index.map(|index| {
        assert!(tile.core_indices.contains(&index), "Tile controller is outside its core_indices");
        index as usize
    });
    let worker_cores = if let Some(controller) = controller_core {
        tile.core_indices.iter().filter(|&&index| index as usize != controller)
            .map(|&index| (c.cores[index as usize].role.clone(), index as usize)).collect()
    } else { Vec::new() };
    TileTopology {
        tile_index,
        controller_core,
        worker_cores,
        endpoint_cores,
        has_buckyball: tile.core_indices.iter().any(|&index| {
            c.cores[index as usize]
                .balldomain
                .as_ref()
                .is_some_and(|domain| domain.ball_num > 0)
        }),
        cores,
        virtual_bank_count: tile.virtual_bank_count as usize,
        shared_physical_bank_count,
        shared_bank_size: bank_entries * (bank_width / 8),
    }
}

pub fn ant_config(core_index: usize) -> Option<&'static AntConfig> {
    let core = &chip().cores[core_index];
    let cpu = core.cpu.as_ref().expect("core CPU config missing");
    match cpu.cpu.as_ref().expect("CPU implementation missing") {
        cpu_config::Cpu::Ant(ant) => {
            assert!(core.hart_id.is_none() && core.ant_context_id.is_some(), "Ant must have only a local context ID");
            Some(ant)
        }
        _ => { assert!(core.hart_id.is_some() && core.ant_context_id.is_none()); None }
    }
}

pub fn tss_config(core_index: usize) -> &'static SpmConfig {
    chip().tiles.iter().find(|tile| tile.core_indices.contains(&(core_index as u32)))
        .expect("core tile missing").tss.as_ref().expect("tile TSS missing")
}

pub fn core_is_ant(core_index: usize) -> bool { ant_config(core_index).is_some() }

pub fn core_hart_id(core_index: usize) -> usize {
    chip().cores[core_index].hart_id.expect("CoreInstance.hart_id is required") as usize
}

pub fn hart_capacity() -> usize {
    let mut harts: Vec<_> = chip().cores.iter().filter_map(|core|
        core.hart_id.map(|id| id as usize)).collect();
    let count = harts.len();
    harts.sort_unstable();
    harts.dedup();
    assert_eq!(harts.len(), count, "configured physical hart IDs must be unique");
    harts.last().expect("chip has no configured harts") + 1
}

pub fn tile_for_core(core_index: usize) -> TileTopology {
    let tile = chip().tiles.iter().position(|tile|
        tile.core_indices.contains(&(core_index as u32))).expect("core belongs to no tile");
    tile_topology(tile)
}

pub fn tile_count() -> usize {
    chip().tiles.len()
}

pub fn core_signature(core_index: usize) -> u64 {
    let core = &chip().cores[core_index];
    let bank = mem_of(core).bank.as_ref().expect("core bank configuration");
    let mut bytes = core.pkg.as_bytes().to_vec();
    bytes.push(0);
    for value in [bank.num, bank.width, bank.entries] {
        bytes.extend_from_slice(&u64::from(value).to_le_bytes());
    }
    let mut isa = core.balldomain.as_ref().map(|domain| domain.isa.iter().collect::<Vec<_>>()).unwrap_or_default();
    isa.sort_by_key(|entry| entry.funct7);
    for entry in isa {
        bytes.extend_from_slice(entry.mnemonic.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&entry.funct7.to_le_bytes());
    }
    let mut mappings = core.balldomain.as_ref().map(|domain| domain.mappings.iter().collect::<Vec<_>>()).unwrap_or_default();
    mappings.sort_by_key(|mapping| mapping.ball_class.rsplit('.').next().unwrap());
    for mapping in mappings {
        bytes.extend_from_slice(mapping.ball_class.rsplit('.').next().unwrap().as_bytes());
        bytes.push(0);
        for value in [mapping.in_bw, mapping.out_bw] {
            bytes.extend_from_slice(&u64::from(value).to_le_bytes());
        }
        let mut params = mapping.ball_params.iter().collect::<Vec<_>>();
        params.sort_by_key(|(name, _)| *name);
        for (name, value) in params {
            bytes.extend_from_slice(name.as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(value.as_bytes());
            bytes.push(0);
        }
    }
    if let Some(ant) = ant_config(core_index) {
        let tile = chip().tiles.iter().find(|tile| tile.core_indices.contains(&core.index)).unwrap();
        let tls = ant.tls.as_ref().expect("Ant TLS missing");
        let tss = tile.tss.as_ref().expect("Ant TSS missing");
        bytes.extend_from_slice(b"ant\0");
        for value in [ant.code_bytes as u64, tls.base, tls.bytes as u64, tls.data_bits as u64,
            ant.task_bits as u64, tss.base, tss.bytes as u64, tss.data_bits as u64,
            tile.shared_mem.as_ref().unwrap().bank_entries as u64, tile.shared_mem.as_ref().unwrap().entries as u64] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes.into_iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

/// Resolve the standard workload profile through the chip's explicit core and tile metadata.
pub fn workload_placement(stem: &str) -> Result<(usize, usize), String> {
    let chip = chip();
    let profiles: Vec<_> = chip.profiles.iter().filter(|profile| {
        ["ctest", "mlirtest", "soctest"].iter().any(|kind|
            stem.starts_with(&format!("{}-{}-{}-", chip.name, profile.name, kind)))
    }).collect();
    let [profile] = profiles.as_slice() else {
        return Err(format!("workload {stem} must identify exactly one configured compiler profile"));
    };
    let core = chip.cores.iter().find(|core| {
        let name = if core.role.is_empty() { &core.pkg } else { &core.role };
        name == &profile.name
    }).ok_or_else(|| format!("profile {} has no configured core", profile.name))?;
    let tile = chip.tiles.iter().position(|tile| tile.core_indices.contains(&core.index))
        .ok_or_else(|| format!("core {} belongs to no tile", core.index))?;
    Ok((tile, core.index as usize))
}
