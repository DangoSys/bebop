use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .ancestors()
        .nth(3)
        .expect("verilator crate should live under bebop/src/nodes/verilator")
        .to_path_buf();
    let bb_root = repo_root
        .parent()
        .expect("bebop repo should live under buckyball root")
        .to_path_buf();
    let arch_dir = bb_root.join("arch");

    // Get config from ARCH_CONFIG environment variable (required)
    let config = env::var("ARCH_CONFIG").expect(
        "ARCH_CONFIG environment variable is required. Example: ARCH_CONFIG=sims.verilator.BuckyballToyVerilatorConfig",
    );

    // Build directory is now arch/build/<config>/
    let build_dir = arch_dir.join("build").join(&config);

    let result_dir = bb_root.join("result");
    let native_dir = manifest_dir.join("native");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let obj_dir = out_dir.join("obj_dir");
    let topname = "BBSimHarness";
    let coverage = env_flag("BEBOP_VERILATOR_COVERAGE");
    let jobs = env::var("NUM_JOBS").unwrap_or_else(|_| "1".to_string());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", native_dir.join("verilator.cc").display());
    // Don't rerun on ARCH_CONFIG change - only rebuild if source files change

    assert_exists(&arch_dir, "missing sibling `arch` repo");

    // Get config from ARCH_CONFIG environment variable (required)
    let config = env::var("ARCH_CONFIG").expect(
        "ARCH_CONFIG environment variable is required. Example: ARCH_CONFIG=sims.verilator.BuckyballToyVerilatorConfig",
    );

    // Build directory is now arch/build/<config>/
    let build_dir = arch_dir.join("build").join(&config);

    assert_exists(
        &build_dir,
        &format!(
            "missing `arch/build/{}`; generate Verilog first with config: {}",
            config, config
        ),
    );
    assert_exists(&result_dir, "missing `result` directory");

    let vsrcs = collect_files(&build_dir, &["v", "sv"]);
    let mut csrcs = collect_files(&arch_dir.join("src/csrc"), &["c", "cc", "cpp"]);
    csrcs.extend(collect_build_csrcs(&build_dir));
    csrcs.retain(|path| should_keep_csrc(path));

    fs::create_dir_all(&obj_dir).expect("create obj_dir");
    // println!("cargo:warning=verilator top={topname}");
    // println!("cargo:warning=verilator obj_dir={}", obj_dir.display());
    // println!("cargo:warning=verilator vsrcs={}", vsrcs.len());
    // println!("cargo:warning=verilator csrcs={}", csrcs.len() + 1);
    run_verilator(&build_dir, &obj_dir, topname, &jobs, coverage, &vsrcs, &csrcs);

    let verilator_root = get_verilator_root(&obj_dir, topname);
    let generated_cpps = collect_files(&obj_dir, &["cpp"]);
    // println!("cargo:warning=verilator generated_cpps={}", generated_cpps.len());

    let mut build = cc::Build::new();
    build.cpp(true);
    build.std("c++17");
    build.warnings(false);
    build.opt_level(3);
    build.flag_if_supported("-fcoroutines");
    build.flag_if_supported("-faligned-new");
    build.flag_if_supported("-fcf-protection=none");
    build.flag_if_supported("-pthread");

    build.define("VM_COVERAGE", if coverage { "1" } else { "0" });
    build.define("VM_SC", "0");
    build.define("VM_TRACE", "1");
    build.define("VM_TRACE_FST", "1");
    build.define("VM_TRACE_VCD", "0");
    build.define("VM_TIMING", "1");

    build.include(&native_dir);
    build.include(result_dir.join("include"));
    build.include(&build_dir);
    build.include(arch_dir.join("src/csrc/include"));
    build.include(&obj_dir);
    build.include(verilator_root.join("include"));
    build.include(verilator_root.join("include/vltstd"));

    // Only compile minimal wrapper + memory model + generated Verilator code
    // All DPI-C callbacks are implemented in Rust (dpi.rs)
    build.file(native_dir.join("verilator.cc"));

    // Include memory model files (mm.cc, mm_dramsim2.cc, BBSimDRAM.cc)
    // DO NOT include monitor/trace files - all DPI-C is in Rust
    for file in &csrcs {
        let file_name = file.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let path_str = file.to_string_lossy();

        // Include memory model files from ioe directory
        if path_str.contains("/src/csrc/src/monitor/ioe/") {
            if file_name.starts_with("BBSimDRAM") || file_name.starts_with("mm") {
                build.file(file);
            }
        }
    }

    for file in &generated_cpps {
        build.file(file);
    }
    for support in verilator_support_sources(&verilator_root, coverage) {
        build.file(support);
    }

    // Add Verilator timing support library (for coroutines)
    build.file(verilator_root.join("include/verilated_timing.cpp"));

    build.compile("bebop_verilator_native");

    // Link against required libraries
    println!("cargo:rustc-link-lib=static=bebop_verilator_native");
    println!("cargo:rustc-link-lib=stdc++"); // C++ standard library

    // Link against DRAMSim2
    let dramsim2_dir = bb_root.join("arch/thirdparty/chipyard/tools/DRAMSim2");
    println!("cargo:rustc-link-search=native={}", dramsim2_dir.display());
    println!("cargo:rustc-link-lib=static=dramsim");

    // Link against zlib (from Nix environment)
    // Add all library paths from NIX_LDFLAGS
    if let Ok(nix_ldflags) = env::var("NIX_LDFLAGS") {
        for flag in nix_ldflags.split_whitespace() {
            if let Some(path) = flag.strip_prefix("-L") {
                println!("cargo:rustc-link-search=native={}", path);
            }
        }
    }

    // Also try to find zlib in common Nix store locations
    let zlib_paths = [
        "/nix/store/8icpg7vrz95c6ap3mznmlmg7h0l2av1w-zlib-1.3.1/lib",
        "/nix/store/ri9paa3mri4kqakljak8ldvbcp7lpmif-zlib-1.3.1/lib",
    ];
    for path in &zlib_paths {
        if Path::new(path).exists() {
            println!("cargo:rustc-link-search=native={}", path);
            break;
        }
    }

    println!("cargo:rustc-link-lib=z"); // zlib for compression
}

fn env_flag(name: &str) -> bool {
    match env::var(name) {
        Ok(value) => value.eq_ignore_ascii_case("true"),
        Err(_) => false,
    }
}

fn assert_exists(path: &Path, message: &str) {
    assert!(path.exists(), "{message}: {}", path.display());
}

fn collect_build_csrcs(build_dir: &Path) -> Vec<PathBuf> {
    collect_files(build_dir, &["c", "cc", "cpp"])
        .into_iter()
        .filter(|path| !path.components().any(|c| c.as_os_str() == OsStr::new("obj_dir")))
        .collect()
}

fn collect_files(root: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_inner(root, exts, &mut files);
    files.sort();
    files
}

fn collect_files_inner(root: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_inner(&path, exts, out);
            continue;
        }
        let Some(ext) = path.extension().and_then(OsStr::to_str) else {
            continue;
        };
        if exts.iter().any(|candidate| candidate.eq_ignore_ascii_case(ext)) {
            out.push(path);
        }
    }
}

fn should_keep_csrc(path: &Path) -> bool {
    let file_name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
    if file_name == "main.cc" && path.to_string_lossy().contains("/src/csrc/src/") {
        return false;
    }
    if file_name == "SimDRAM.cc" && !path.to_string_lossy().contains("/src/csrc/") {
        panic!("unexpected testchipip SimDRAM.cc in BBSim build: {}", path.display());
    }
    if matches!(file_name, "testchip_tsi.cc" | "testchip_htif.cc" | "SimTSI.cc") {
        panic!("unexpected TSI source in BBSim build: {}", path.display());
    }
    true
}

fn pkg_config_var(pkg: &str, var: &str) -> Option<String> {
    let output = Command::new("pkg-config")
        .arg(format!("--variable={var}"))
        .arg(pkg)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    Some(value.trim().to_string())
}

fn get_verilator_root(obj_dir: &Path, topname: &str) -> PathBuf {
    let mk = obj_dir.join(format!("V{topname}.mk"));
    let contents = fs::read_to_string(&mk).expect("read generated V*.mk");
    let line = contents
        .lines()
        .find(|line| line.starts_with("VERILATOR_ROOT = "))
        .expect("VERILATOR_ROOT line");
    PathBuf::from(line.trim_start_matches("VERILATOR_ROOT = ").trim())
}

fn verilator_support_sources(verilator_root: &Path, coverage: bool) -> Vec<PathBuf> {
    let include = verilator_root.join("include");
    let mut files = vec![
        include.join("verilated.cpp"),
        include.join("verilated_dpi.cpp"),
        include.join("verilated_vpi.cpp"),
        include.join("verilated_fst_c.cpp"),
        include.join("verilated_threads.cpp"),
    ];
    if coverage {
        files.push(include.join("verilated_cov.cpp"));
    }
    files
}

fn run_verilator(
    build_dir: &Path,
    obj_dir: &Path,
    topname: &str,
    jobs: &str,
    coverage: bool,
    vsrcs: &[PathBuf],
    csrcs: &[PathBuf],
) {
    let mut cmd = Command::new("verilator");
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    cmd.arg("-MMD")
        .arg("-cc")
        .arg("--vpi")
        .arg("--trace")
        .arg("-O3")
        .arg("--x-assign")
        .arg("fast")
        .arg("--x-initial")
        .arg("fast")
        .arg("--noassert")
        .arg("-Wno-fatal")
        .arg("--trace-fst")
        .arg("--trace-threads")
        .arg("1")
        .arg("--output-split")
        .arg("10000")
        .arg("--output-split-cfuncs")
        .arg("100")
        .arg("--unroll-count")
        .arg("256")
        .arg("-Wno-PINCONNECTEMPTY")
        .arg("-Wno-ASSIGNDLY")
        .arg("-Wno-DECLFILENAME")
        .arg("-Wno-UNUSED")
        .arg("-Wno-UNOPTFLAT")
        .arg("-Wno-BLKANDNBLK")
        .arg("-Wno-style")
        .arg("-Wall")
        .arg("--timing")
        .arg("-j")
        .arg(jobs)
        .arg(format!("+incdir+{}", build_dir.display()))
        .arg("--top")
        .arg(topname)
        .arg("--Mdir")
        .arg(obj_dir);

    if coverage {
        cmd.arg("--coverage-line");
    }

    for src in vsrcs {
        cmd.arg(src);
    }
    for src in csrcs {
        cmd.arg(src);
    }

    let status = cmd.status().expect("run verilator");
    if !status.success() {
        panic!("verilator failed with status {status}");
    }
}
