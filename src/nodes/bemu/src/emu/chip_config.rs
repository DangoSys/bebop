use prost::Message;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/buckyball.config.rs"));

const CHIP_PB: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../chip.pb"));

#[derive(Clone)]
pub struct Topology {
    pub mem_config: MemConfig,
    pub ball_domain: BallDomainConfig,
    pub vector_len: usize,
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
    Topology {
        virtual_bank_count: virtual_bank_count_for_core(core.index as usize),
        vector_len: core.gp_domain.as_ref().map_or(0, |gp_domain| gp_domain.v_len as usize),
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
    if tile.virtual_bank_count == 0 {
        panic!("tile {tile_index} virtual_bank_count is 0");
    }
    let shared = tile
        .shared_mem
        .as_ref()
        .unwrap_or_else(|| panic!("tile {tile_index} missing shared_mem"));
    let first_core = &c.cores[tile.core_indices[0] as usize];
    let bank_entries = mem_of(first_core)
        .bank
        .as_ref()
        .unwrap_or_else(|| panic!("core {} missing bank", first_core.index))
        .entries as usize;
    let bank_width = mem_of(first_core)
        .bank
        .as_ref()
        .unwrap_or_else(|| panic!("core {} missing bank", first_core.index))
        .width as usize;
    assert_eq!(bank_width % 8, 0, "tile {tile_index} bank width is not byte-aligned");
    for &core_index in &tile.core_indices {
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
    TileTopology {
        has_buckyball: tile.core_indices.iter().any(|&index| {
            c.cores[index as usize]
                .balldomain
                .as_ref()
                .is_some_and(|domain| !domain.mappings.is_empty())
        }),
        cores,
        virtual_bank_count: tile.virtual_bank_count as usize,
        shared_physical_bank_count,
        shared_bank_size: bank_entries * (bank_width / 8),
    }
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
    let mut isa = core
        .balldomain
        .as_ref()
        .expect("core instruction set")
        .isa
        .iter()
        .collect::<Vec<_>>();
    isa.sort_by_key(|entry| entry.funct7);
    for entry in isa {
        bytes.extend_from_slice(entry.mnemonic.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&entry.funct7.to_le_bytes());
    }
    let mut mappings = core
        .balldomain
        .as_ref()
        .expect("core instruction set")
        .mappings
        .iter()
        .collect::<Vec<_>>();
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
    bytes.into_iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
