use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const COLD_LOAD_CAPABILITY: &str = "p2e-cold-load-v1";
const DDR_BASE: u64 = 0x8000_0000;
const DDR_SIZE: u64 = 16 * 1024 * 1024 * 1024;
const FDT_SIZE: u64 = 256 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LoadEntry {
    pub role: String,
    pub file: PathBuf,
    pub offset: u64,
    pub size: u64,
    pub sha256: String,
    pub format: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LoadManifest {
    pub version: u32,
    pub ddr_base: u64,
    pub ddr_size: u64,
    pub guest_memory_bytes: u64,
    pub pmem_base: u64,
    pub pmem_size: u64,
    pub fdt_base: u64,
    pub fdt_size: u64,
    pub loads: Vec<LoadEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadPlan {
    pub capability: String,
    pub manifest_path: Option<PathBuf>,
    pub manifest_sha256: Option<String>,
    pub manifest: Option<LoadManifest>,
    pub loads: Vec<LoadEntry>,
}

pub fn validate_cold_case(case: &Path) -> Result<(), String> {
    let file = case.join("p2e-cold-load.cap");
    let capability = std::fs::read_to_string(&file)
        .map_err(|e| format!("cold-load case capability missing: {}: {e}", file.display()))?;
    if capability != format!("{COLD_LOAD_CAPABILITY}\n") {
        return Err(format!("unsupported cold-load case capability: {}", file.display()));
    }
    Ok(())
}

pub fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut digest = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let count = file
            .read(&mut bytes)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        if count == 0 {
            break;
        }
        digest.update(&bytes[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn end(base: u64, size: u64, name: &str) -> Result<u64, String> {
    base.checked_add(size).ok_or_else(|| format!("{name} range overflows"))
}

pub fn validate_loads(image: Option<&Path>, manifest_path: Option<&Path>) -> Result<LoadPlan, String> {
    match (image, manifest_path) {
        (Some(image), None) => {
            let file = image
                .canonicalize()
                .map_err(|e| format!("image {}: {e}", image.display()))?;
            let metadata = std::fs::metadata(&file).map_err(|e| e.to_string())?;
            if !metadata.is_file() || metadata.len() == 0 || file.extension().and_then(|x| x.to_str()) != Some("hex") {
                return Err("legacy image must be a nonempty regular hex file".into());
            }
            let entry = LoadEntry {
                role: "boot".into(),
                sha256: file_sha256(&file)?,
                file,
                offset: 0,
                size: metadata.len(),
                format: "hex".into(),
            };
            Ok(LoadPlan {
                capability: COLD_LOAD_CAPABILITY.into(),
                manifest_path: None,
                manifest_sha256: None,
                manifest: None,
                loads: vec![entry],
            })
        }
        (None, Some(path)) => {
            let path = path
                .canonicalize()
                .map_err(|e| format!("manifest {}: {e}", path.display()))?;
            let bytes = std::fs::read(&path).map_err(|e| format!("read manifest: {e}"))?;
            let mut manifest: LoadManifest =
                serde_json::from_slice(&bytes).map_err(|e| format!("invalid load manifest: {e}"))?;
            if manifest.version != 1 || manifest.ddr_base != DDR_BASE || manifest.ddr_size != DDR_SIZE {
                return Err("load manifest requires version 1 and DDR base 0x80000000 / size 16GiB".into());
            }
            let guest = manifest.guest_memory_bytes;
            if guest <= FDT_SIZE || guest >= DDR_SIZE || guest % (1024 * 1024) != 0 {
                return Err("guest_memory_bytes must be MiB-aligned and leave guest/FDT and pmem regions".into());
            }
            if manifest.pmem_base != end(DDR_BASE, guest, "pmem base")?
                || manifest.pmem_size != DDR_SIZE - guest
                || manifest.fdt_size != FDT_SIZE
                || manifest.fdt_base != end(DDR_BASE, guest - FDT_SIZE, "FDT base")?
            {
                return Err("pmem/FDT fields do not match the DDR and guest memory partition".into());
            }
            end(manifest.ddr_base, manifest.ddr_size, "DDR")?;
            end(manifest.pmem_base, manifest.pmem_size, "pmem")?;
            end(manifest.fdt_base, manifest.fdt_size, "FDT")?;
            if manifest.loads.len() != 2 {
                return Err("load manifest requires exactly boot and model loads".into());
            }
            manifest.loads.sort_by_key(|load| load.offset);
            let boot = &manifest.loads[0];
            let model = &manifest.loads[1];
            if boot.role != "boot"
                || boot.offset != 0
                || boot.size == 0
                || end(boot.offset, boot.size, "boot")? > guest - FDT_SIZE
                || model.role != "model"
                || model.offset != guest
                || model.size == 0
                || end(model.offset, model.size, "model")? > DDR_SIZE
            {
                return Err("boot/model roles or offsets/sizes overlap reserved memory or exceed DDR".into());
            }
            for load in &mut manifest.loads {
                log::info!(
                    "Validating {} load: {} bytes, {}",
                    load.role,
                    load.size,
                    load.file.display()
                );
                if load.format != "binary"
                    || load.sha256.len() != 64
                    || !load
                        .sha256
                        .bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                {
                    return Err("loads require binary format and a lowercase SHA256 digest".into());
                }
                let file = if load.file.is_absolute() {
                    load.file.clone()
                } else {
                    path.parent().ok_or("manifest has no parent")?.join(&load.file)
                };
                load.file = file
                    .canonicalize()
                    .map_err(|e| format!("load {}: {e}", file.display()))?;
                let metadata = std::fs::metadata(&load.file).map_err(|e| e.to_string())?;
                if !metadata.is_file() || metadata.len() != load.size {
                    return Err(format!("load size mismatch: {}", load.file.display()));
                }
                if file_sha256(&load.file)? != load.sha256 {
                    return Err(format!("load SHA256 mismatch: {}", load.file.display()));
                }
            }
            Ok(LoadPlan {
                capability: COLD_LOAD_CAPABILITY.into(),
                manifest_path: Some(path),
                manifest_sha256: Some(format!("{:x}", Sha256::digest(&bytes))),
                loads: manifest.loads.clone(),
                manifest: Some(manifest),
            })
        }
        _ => Err("exactly one of --image and --load-manifest is required".into()),
    }
}
