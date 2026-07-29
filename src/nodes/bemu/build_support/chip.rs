use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::config_loader::Topology;

const SYSTEM_FUNCTS: &[u32] = &[0, 1, 16, 32, 33, 34, 35];

pub fn validate_ball_set(manifest_dir: &Path, topology: &Topology) -> Vec<PathBuf> {
    let chip_rs = manifest_dir.join("src/chip.rs");
    let chip_source =
        fs::read_to_string(&chip_rs).unwrap_or_else(|e| panic!("failed to read {}: {e}", chip_rs.display()));
    let chip_dir = chip_rs.parent().expect("chip.rs has parent");

    let mut ball_files = Vec::new();
    let mut chip_classes = BTreeSet::new();
    for rel in path_attributes(&chip_source, &chip_rs) {
        let path = resolve_path(chip_dir, &rel);
        let source = fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let ball_class = parse_ball_class_const(&source, &path)
            .unwrap_or_else(|| panic!("{} must define BALL_CLASS for BEMU chip registration", path.display()));
        if !chip_classes.insert(ball_class.clone()) {
            panic!(
                "{} registers duplicate BEMU ballClass {}",
                chip_rs.display(),
                ball_class
            );
        }
        ball_files.push(path);
    }

    let mut toml_classes = BTreeSet::new();
    for entry in &topology.ball_domain.isa {
        if SYSTEM_FUNCTS.contains(&entry.funct7) {
            continue;
        }
        let mapping = topology
            .ball_domain
            .mappings
            .iter()
            .find(|mapping| mapping.ball_id == entry.bid)
            .unwrap_or_else(|| panic!("ballISA funct7 {} references missing bid {}", entry.funct7, entry.bid));
        toml_classes.insert(mapping.ball_class.clone());
    }

    let missing: Vec<_> = toml_classes.difference(&chip_classes).cloned().collect();
    let extra: Vec<_> = chip_classes.difference(&toml_classes).cloned().collect();
    if !missing.is_empty() || !extra.is_empty() {
        panic!(
            "{} BEMU ball registration must exactly match TOML ballIdMappings. missing_in_chip={:?} extra_in_chip={:?}",
            chip_rs.display(),
            missing,
            extra
        );
    }

    ball_files
}

fn path_attributes(source: &str, path: &Path) -> Vec<String> {
    let mut attrs = Vec::new();
    let mut rest = source;
    while let Some(pos) = rest.find("#[path") {
        rest = &rest[pos + "#[path".len()..];
        let start = rest
            .find('"')
            .unwrap_or_else(|| panic!("{} has #[path] without string literal", path.display()));
        let value = &rest[start + 1..];
        let end = value
            .find('"')
            .unwrap_or_else(|| panic!("{} has unterminated #[path] string literal", path.display()));
        attrs.push(value[..end].to_string());
        rest = &value[end + 1..];
    }
    attrs
}

fn parse_ball_class_const(source: &str, path: &Path) -> Option<String> {
    let pos = source.find("BALL_CLASS")?;
    let rest = &source[pos..];
    let start = rest
        .find('"')
        .unwrap_or_else(|| panic!("{} BALL_CLASS must be a string literal", path.display()));
    let value = &rest[start + 1..];
    let end = value
        .find('"')
        .unwrap_or_else(|| panic!("{} BALL_CLASS string literal is unterminated", path.display()));
    Some(value[..end].to_string())
}

fn resolve_path(base: &Path, rel: &str) -> PathBuf {
    let path = Path::new(rel);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}
