use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub struct MemConfig {
    pub bank_num: usize,
    pub bank_width: usize,
    pub bank_entries: usize,
    pub mmio_enable: bool,
    pub mmio_bank_num: usize,
    pub mmio_bank_entries: usize,
    pub mmio_bank_width: usize,
    pub mmio_read_width: usize,
}

impl Default for MemConfig {
    fn default() -> Self {
        Self {
            bank_num: 32,
            bank_width: 128,
            bank_entries: 1024,
            mmio_enable: true,
            mmio_bank_num: 16,
            mmio_bank_entries: 64,
            mmio_bank_width: 128,
            mmio_read_width: 8,
        }
    }
}

pub struct Topology {
    pub mem_config: MemConfig,
    pub ball_domain: BallDomainConfig,
    pub files_read: Vec<PathBuf>,
}

#[derive(Clone)]
pub struct BallDomainConfig {
    pub mappings: Vec<BallIdMapping>,
    pub isa: Vec<BallIsaEntry>,
}

impl BallDomainConfig {
    pub fn empty() -> Self {
        Self {
            mappings: Vec::new(),
            isa: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct BallIdMapping {
    pub ball_id: u32,
    pub ball_class: String,
}

#[derive(Clone)]
pub struct BallIsaEntry {
    pub funct7: u32,
    pub bid: u32,
}

pub fn top_config_from_manifest(manifest_dir: &Path) -> PathBuf {
    let chip_lib = manifest_dir.join("src").join("lib.rs");
    let source = fs::read_to_string(&chip_lib).unwrap_or_else(|e| panic!("failed to read {}: {e}", chip_lib.display()));
    let rel = parse_top_config_const(&source, &chip_lib)
        .unwrap_or_else(|| panic!("{} must define BEMU_TOP_CONFIG", chip_lib.display()));
    resolve_path(chip_lib.parent().expect("chip lib has parent"), &rel)
}

fn parse_top_config_const(source: &str, path: &Path) -> Option<String> {
    let pos = source.find("BEMU_TOP_CONFIG")?;
    let rest = &source[pos..];
    let start = rest
        .find('"')
        .unwrap_or_else(|| panic!("{} BEMU_TOP_CONFIG must be a string literal", path.display()));
    let value = &rest[start + 1..];
    let end = value
        .find('"')
        .unwrap_or_else(|| panic!("{} BEMU_TOP_CONFIG string literal is unterminated", path.display()));
    Some(value[..end].to_string())
}

pub fn parse_topology(top_config: &Path) -> Topology {
    let mut files_read = Vec::new();
    let top = parse_toml_file_record(top_config, &mut files_read);
    let top_dir = top_config.parent().expect("top config has parent");

    let tile_path = first_include_path(&top, "tiles")
        .unwrap_or_else(|| panic!("{} must define [[tiles]].include", top_config.display()));
    let tile_path = resolve_path(top_dir, &tile_path);
    let tile = parse_toml_file_record(&tile_path, &mut files_read);
    let tile_dir = tile_path.parent().expect("tile config has parent");

    let core_path = first_include_path(&tile, "cores")
        .or_else(|| table_include_path(&tile, "coreTemplate"))
        .unwrap_or_else(|| {
            panic!(
                "{} must define [[cores]].include or [coreTemplate].include",
                tile_path.display()
            )
        });
    let core_path = resolve_path(tile_dir, &core_path);
    let core = parse_toml_file_record(&core_path, &mut files_read);
    let core_dir = core_path.parent().expect("core config has parent");

    let memdomain_path =
        string_key(&core, "memdomain").unwrap_or_else(|| panic!("{} must define memdomain", core_path.display()));
    let memdomain_path = resolve_path(core_dir, &memdomain_path);
    let memdomain = parse_toml_file_record(&memdomain_path, &mut files_read);

    let balldomain_path =
        string_key(&core, "balldomain").unwrap_or_else(|| panic!("{} must define balldomain", core_path.display()));
    let balldomain_path = resolve_path(core_dir, &balldomain_path);
    let balldomain = parse_toml_file_record(&balldomain_path, &mut files_read);

    Topology {
        mem_config: parse_mem_config(&memdomain, &memdomain_path),
        ball_domain: parse_ball_domain(&balldomain, &balldomain_path),
        files_read,
    }
}

fn parse_mem_config(value: &toml::Value, path: &Path) -> MemConfig {
    let bank = table(value, "bank", path);
    let mmio = table(value, "mmio", path);
    MemConfig {
        bank_num: get_usize(bank, "num", path),
        bank_width: get_usize(bank, "width", path),
        bank_entries: get_usize(bank, "entries", path),
        mmio_enable: get_bool(mmio, "enable", path),
        mmio_bank_num: get_usize(mmio, "bankNum", path),
        mmio_bank_entries: get_usize(mmio, "bankEntries", path),
        mmio_bank_width: get_usize(mmio, "bankWidth", path),
        mmio_read_width: get_usize(mmio, "readWidth", path),
    }
}

fn first_include_path(value: &toml::Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_array()?
        .first()
        .and_then(|entry| string_key(entry, "include"))
}

fn parse_ball_domain(value: &toml::Value, path: &Path) -> BallDomainConfig {
    let mappings: Vec<_> = value
        .get("ballIdMappings")
        .and_then(toml::Value::as_array)
        .unwrap_or_else(|| panic!("{} must define ballIdMappings", path.display()))
        .iter()
        .map(|entry| {
            let ball_id = entry
                .get("ballId")
                .and_then(toml::Value::as_integer)
                .unwrap_or_else(|| panic!("{} ballIdMappings entry must define integer ballId", path.display()));
            let ball_class = entry
                .get("ballClass")
                .and_then(toml::Value::as_str)
                .unwrap_or_else(|| panic!("{} ballIdMappings entry must define ballClass", path.display()));
            BallIdMapping {
                ball_id: u32::try_from(ball_id)
                    .unwrap_or_else(|_| panic!("{} ballId must be non-negative", path.display())),
                ball_class: ball_class.to_string(),
            }
        })
        .collect();

    let isa: Vec<_> = value
        .get("ballISA")
        .and_then(toml::Value::as_array)
        .unwrap_or_else(|| panic!("{} must define ballISA", path.display()))
        .iter()
        .map(|entry| {
            let funct7 = entry
                .get("funct7")
                .and_then(toml::Value::as_integer)
                .unwrap_or_else(|| panic!("{} ballISA entry must define integer funct7", path.display()));
            let bid = entry
                .get("bid")
                .and_then(toml::Value::as_integer)
                .unwrap_or_else(|| panic!("{} ballISA entry must define integer bid", path.display()));
            BallIsaEntry {
                funct7: u32::try_from(funct7)
                    .unwrap_or_else(|_| panic!("{} ballISA funct7 must be non-negative", path.display())),
                bid: u32::try_from(bid)
                    .unwrap_or_else(|_| panic!("{} ballISA bid must be non-negative", path.display())),
            }
        })
        .collect();

    validate_ball_domain(&mappings, &isa, path);

    BallDomainConfig { mappings, isa }
}

fn validate_ball_domain(mappings: &[BallIdMapping], isa: &[BallIsaEntry], path: &Path) {
    let mut ball_ids = BTreeSet::new();
    for mapping in mappings {
        if !ball_ids.insert(mapping.ball_id) {
            panic!("{} has duplicate ballId {}", path.display(), mapping.ball_id);
        }
    }

    let mut functs = BTreeSet::new();
    for entry in isa {
        if !functs.insert(entry.funct7) {
            panic!("{} has duplicate ballISA funct7 {}", path.display(), entry.funct7);
        }
        if !ball_ids.contains(&entry.bid) {
            panic!(
                "{} ballISA funct7 {} references missing bid {}",
                path.display(),
                entry.funct7,
                entry.bid
            );
        }
    }
}

fn table_include_path(value: &toml::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|entry| string_key(entry, "include"))
}

fn table<'a>(value: &'a toml::Value, key: &str, path: &Path) -> &'a toml::map::Map<String, toml::Value> {
    value
        .get(key)
        .and_then(toml::Value::as_table)
        .unwrap_or_else(|| panic!("{} must define [{key}]", path.display()))
}

fn get_usize(table: &toml::map::Map<String, toml::Value>, key: &str, path: &Path) -> usize {
    let value = table
        .get(key)
        .and_then(toml::Value::as_integer)
        .unwrap_or_else(|| panic!("{} must define integer key {key}", path.display()));
    usize::try_from(value).unwrap_or_else(|_| panic!("{} key {key} must be non-negative", path.display()))
}

fn get_bool(table: &toml::map::Map<String, toml::Value>, key: &str, path: &Path) -> bool {
    table
        .get(key)
        .and_then(toml::Value::as_bool)
        .unwrap_or_else(|| panic!("{} must define boolean key {key}", path.display()))
}

fn string_key(value: &toml::Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(ToString::to_string)
}

fn parse_toml_file_record(path: &Path, files_read: &mut Vec<PathBuf>) -> toml::Value {
    files_read.push(path.to_path_buf());
    parse_toml_file(path)
}

fn parse_toml_file(path: &Path) -> toml::Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    text.parse::<toml::Value>()
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()))
}

fn resolve_path(base: &Path, rel: &str) -> PathBuf {
    let path = Path::new(rel);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}
