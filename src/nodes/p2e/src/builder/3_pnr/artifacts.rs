use std::path::{Path, PathBuf};

fn nonempty(path: &Path) -> Result<(), String> {
    let metadata = path
        .metadata()
        .map_err(|e| format!("Required P2E artifact {}: {e}", path.display()))?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(format!(
            "Required P2E artifact is not a nonempty file: {}",
            path.display()
        ));
    }
    Ok(())
}

pub fn partitions(case: &Path) -> Result<Vec<String>, String> {
    let root = case.join("fpgaCompDir");
    let mut parts = Vec::new();
    for entry in std::fs::read_dir(&root).map_err(|e| format!("{}: {e}", root.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("part_b") {
            continue;
        }
        let valid = name
            .strip_prefix("part_b")
            .and_then(|s| s.split_once("_f"))
            .is_some_and(|(b, f)| {
                !b.is_empty() && !f.is_empty() && b.bytes().chain(f.bytes()).all(|c| c.is_ascii_digit())
            });
        if !valid || !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            return Err(format!("Invalid P2E partition: {name}"));
        }
        nonempty(&entry.path().join("Makefile"))?;
        parts.push(name);
    }
    parts.sort();
    if !parts.iter().any(|p| p == "part_b0_f0") {
        return Err("P2E DDR partition part_b0_f0 is missing".into());
    }
    Ok(parts)
}

pub fn bitstreams(case: &Path, parts: &[String]) -> Result<PathBuf, String> {
    let mut primary = None;
    for part in parts {
        let directory = case.join("fpgaCompDir").join(part);
        let makefile = std::fs::read_to_string(directory.join("Makefile")).map_err(|e| e.to_string())?;
        let top = makefile
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once('=')?;
                (key.trim() == "export FPGA_TOP").then(|| value.trim())
            })
            .ok_or_else(|| format!("{part}: missing FPGA_TOP in generated Makefile"))?;
        if top.is_empty() || !top.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
            return Err(format!("{part}: invalid FPGA_TOP {top:?}"));
        }
        for extension in ["bit", "bin"] {
            nonempty(&directory.join("pnrDir").join(format!("{top}.{extension}")))?;
        }
        if part == "part_b0_f0" {
            primary = Some(directory.join("pnrDir").join(format!("{top}.bit")));
        }
    }
    primary.ok_or_else(|| "P2E DDR partition part_b0_f0 is missing".into())
}

pub fn runtime(case: &Path) -> Result<(), String> {
    // Structural runtime databases are mandatory; optional trace/readback tables may be absent.
    for file in [
        "hierarchy.db",
        "physical_data.db",
        "physical_name.db",
        "physical_tree.db",
        "xndb_hier.db",
        "xndb_inst.db",
        "xndb_module.db",
        "xndb_name.db",
        "xndb_tree.db",
        "xndb_uuid.db",
    ] {
        nonempty(&case.join("RTDB").join(file))?;
    }
    for file in ["vvacDir/runtimeDir/rtcfg", "vvacDir/runtimeDir/lib/lib_arm/libvCtb.so"] {
        nonempty(&case.join(file))?;
    }
    Ok(())
}

pub fn sta_command(case: &Path) -> Result<String, String> {
    let makefile = std::fs::read_to_string(case.join("fpgaCompDir/Makefile")).map_err(|e| e.to_string())?;
    let mut lines = makefile.lines().skip_while(|line| *line != "sta_report:").skip(1);
    let mut recipe = Vec::new();
    for line in lines.by_ref() {
        if line.is_empty() {
            continue;
        }
        if !line.starts_with('\t') {
            break;
        }
        recipe.push(line.trim());
    }
    // This target has one shared command followed by per-partition refresh jobs.
    // Fail on a new SDK recipe instead of silently omitting new prerequisites.
    if recipe.len() != 2
        || !recipe[0].starts_with("${VCOM_HOME}/bin/postPrTiming ")
        || !recipe[1].contains(" reg_init_refresh ")
        || !recipe[1].ends_with("wait")
    {
        return Err("Unsupported generated sta_report recipe".into());
    }
    Ok(recipe[0].to_owned())
}
