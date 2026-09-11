use bebop_rushb::decode_core_id;
use prost::Message;
use std::path::PathBuf;

include!(concat!(env!("OUT_DIR"), "/buckyball.config.rs"));

const CHIP_PB: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../chip.pb"));

#[derive(Clone)]
pub struct Topology {
    pub mem_config: MemConfig,
    pub ball_domain: BallDomainConfig,
    pub vector_len: usize,
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
    pub virtual_bank_count: usize,
    pub shared_physical_bank_count: usize,
    pub shared_bank_size: usize,
}

pub struct RushBEndpoint {
    pub core_index: usize,
    pub virtual_bank_count: usize,
}

fn chip() -> Chip {
    Chip::decode(CHIP_PB).unwrap_or_else(|e| panic!("decode chip.pb: {e}"))
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

pub fn topology_for_core(core_index: usize) -> Topology {
    let c = chip();
    let core = c.cores.get(core_index).unwrap_or_else(|| {
        panic!(
            "core index {core_index} out of range (n={})",
            c.cores.len()
        )
    });
    to_topology(core)
}

pub fn virtual_bank_count_for_core(core_index: usize) -> usize {
    let c = chip();
    let mut count = None;
    for tile in &c.tiles {
        if tile.core_indices.iter().any(|&index| index as usize == core_index) {
            assert!(count.is_none(), "core {core_index} belongs to multiple tiles");
            assert!(tile.virtual_bank_count > 0, "core {core_index} tile has no virtual banks");
            count = Some(tile.virtual_bank_count as usize);
        }
    }
    count.unwrap_or_else(|| panic!("core {core_index} belongs to no tile"))
}

pub fn rushb_endpoint(core_id: u32) -> RushBEndpoint {
    let (tile_id, local_id) = decode_core_id(core_id);
    let c = chip();
    let tile = c.tiles.get(tile_id as usize).unwrap_or_else(|| {
        panic!(
            "rushB Core {core_id}: tile {tile_id} out of range (n={})",
            c.tiles.len()
        )
    });
    let core_index = *tile.core_indices.get(local_id as usize).unwrap_or_else(|| {
        panic!(
            "rushB Core {core_id}: local index {local_id} out of range for tile {tile_id} (n={})",
            tile.core_indices.len()
        )
    }) as usize;
    let core = c.cores.get(core_index).unwrap_or_else(|| {
        panic!(
            "rushB Core {core_id}: config index {core_index} out of range (n={})",
            c.cores.len()
        )
    });
    if core
        .balldomain
        .as_ref()
        .map_or(true, |ball| ball.mappings.is_empty())
    {
        panic!("rushB Core {core_id}: config index {core_index} has no Buckyball mappings");
    }
    if tile.virtual_bank_count == 0 {
        panic!("rushB Core {core_id}: tile {tile_id} virtual_bank_count is 0");
    }
    RushBEndpoint {
        core_index,
        virtual_bank_count: tile.virtual_bank_count as usize,
    }
}

pub fn tile_topology(tile_index: usize) -> TileTopology {
    let c = chip();
    let tile = c.tiles.get(tile_index).unwrap_or_else(|| {
        panic!(
            "tile index {tile_index} out of range (n={})",
            c.tiles.len()
        )
    });
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
        assert_eq!(bank.width as usize, bank_width, "tile {tile_index} cores have different bank widths");
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
        cores,
        virtual_bank_count: tile.virtual_bank_count as usize,
        shared_physical_bank_count,
        shared_bank_size: bank_entries * (bank_width / 8),
    }
}

pub fn chip_manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
