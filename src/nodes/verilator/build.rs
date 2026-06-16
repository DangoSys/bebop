//===- build.rs - Build Bebop Verilator for RTL simulation -----------------===//
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
//===-----------------------------------------------------------------===//-----===//
//
//
//
//===-----------------------------------------------------------------===//-----===//

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let native_dir = manifest_dir.join("native");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let obj_dir = out_dir.join("obj_dir");

    let build_dir = resolve_vsrc_path(&manifest_dir);
    let vsrc_path = build_dir.display().to_string();

    let topname = "BBSimHarness";
    let coverage = env_flag("BEBOP_VERILATOR_COVERAGE");
    let jobs = capped_jobs();

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", native_dir.join("verilator.cc").display());
    println!("cargo:rerun-if-changed={}", native_dir.join("memory").display());
    println!("cargo:rerun-if-changed={}", build_dir.display());
    println!("cargo:rerun-if-env-changed=VSRC_PATH");
    println!("cargo:rerun-if-env-changed=ARCH_CONFIG");
    println!("cargo:rerun-if-env-changed=BEBOP_VERILATOR_COVERAGE");

    assert_exists(&build_dir, &format!("missing Verilog source directory: {}", vsrc_path));

    let vsrcs = collect_files(&build_dir, &["v", "sv"]);
    let csrcs = collect_build_csrcs(&build_dir);

    if obj_dir.exists() {
        fs::remove_dir_all(&obj_dir).expect("remove stale obj_dir");
    }
    fs::create_dir_all(&obj_dir).expect("create obj_dir");
    run_verilator(&build_dir, &obj_dir, topname, &jobs, coverage, &vsrcs, &csrcs);

    let verilator_root = get_verilator_root(&obj_dir, topname);
    let generated_cpps = collect_files(&obj_dir, &["cpp"]);

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
    build.include(native_dir.join("include"));
    build.include(&build_dir);
    build.include(&obj_dir);
    build.include(verilator_root.join("include"));
    build.include(verilator_root.join("include/vltstd"));

    // Add include paths from NIX_CFLAGS_COMPILE (set by nix-shell)
    if let Ok(nix_cflags) = env::var("NIX_CFLAGS_COMPILE") {
        for flag in nix_cflags.split_whitespace() {
            if let Some(path) = flag.strip_prefix("-I") {
                let include_path = PathBuf::from(path);
                if include_path.exists() {
                    build.include(&include_path);
                }
            }
        }
    }

    // Add include paths from NIX_LDFLAGS (derive include from lib paths)
    if let Ok(nix_ldflags) = env::var("NIX_LDFLAGS") {
        for flag in nix_ldflags.split_whitespace() {
            if let Some(path) = flag.strip_prefix("-L") {
                let include_path = PathBuf::from(path).parent().unwrap().join("include");
                if include_path.exists() {
                    build.include(&include_path);
                }
            }
        }
    }

    // Fallback: search Nix store for spike and dramsim2 include paths
    // Nix store directory names follow the pattern: <hash>-<name>-<version>
    if let Ok(entries) = fs::read_dir("/nix/store") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            let include_dir = entry.path().join("include");
            if !include_dir.exists() {
                continue;
            }
            // Match spike packages (e.g. "spike-1.1.0-unstable-2024-09-21")
            if name_str.contains("-spike-") {
                build.include(&include_dir);
            }
            // Match dramsim2 packages (e.g. "dramsim2-2023-05-10")
            if name_str.contains("-dramsim2-") {
                build.include(&include_dir);
            }
            // Match zlib dev packages (e.g. "zlib-1.3.1-dev")
            if name_str.contains("-zlib-") && name_str.contains("-dev") {
                build.include(&include_dir);
            }
        }
    }

    // Compile minimal wrapper + memory model + generated Verilator code
    // All DPI-C callbacks are implemented in Rust (dpi.rs)
    let native_csrcs = [
        native_dir.join("verilator.cc"),
        native_dir.join("memory/BBSimDRAM.cc"),
        native_dir.join("memory/mm.cc"),
        native_dir.join("memory/mm_dramsim2.cc"),
    ];
    for src in native_csrcs {
        println!("cargo:rerun-if-changed={}", src.display());
        build.file(src);
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
    println!("cargo:rustc-link-lib=stdc++");

    // Add library search paths from NIX_LDFLAGS
    if let Ok(nix_ldflags) = env::var("NIX_LDFLAGS") {
        for flag in nix_ldflags.split_whitespace() {
            if let Some(path) = flag.strip_prefix("-L") {
                println!("cargo:rustc-link-search=native={}", path);
            }
        }
    }

    // Fallback: search Nix store for dramsim2 and zlib lib paths
    if let Ok(entries) = fs::read_dir("/nix/store") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            let lib_dir = entry.path().join("lib");
            if !lib_dir.exists() {
                continue;
            }
            if name_str.contains("-dramsim2-") {
                println!("cargo:rustc-link-search=native={}", lib_dir.display());
            }
            if name_str.contains("-zlib-") && !name_str.contains("-dev") {
                println!("cargo:rustc-link-search=native={}", lib_dir.display());
            }
        }
    }

    println!("cargo:rustc-link-lib=dylib=dramsim");
    println!("cargo:rustc-link-lib=z");
}

fn resolve_vsrc_path(manifest_dir: &Path) -> PathBuf {
    if let Ok(path) = env::var("VSRC_PATH") {
        return PathBuf::from(path);
    }

    let arch_config =
        env::var("ARCH_CONFIG").unwrap_or_else(|_| "sims.verilator.BuckyballToyVerilatorConfig".to_string());
    for ancestor in manifest_dir.ancestors() {
        let candidate = ancestor.join("arch").join("build").join(&arch_config);
        if candidate.exists() {
            println!(
                "cargo:warning=VSRC_PATH not set; using inferred Verilator build directory {}",
                candidate.display()
            );
            return candidate;
        }
    }

    panic!(
        "VSRC_PATH environment variable is required, or ARCH_CONFIG must resolve under an ancestor arch/build directory. \
         Example: VSRC_PATH=arch/build/sims.verilator.BuckyballToyVerilatorConfig"
    );
}

fn capped_jobs() -> String {
    let jobs = env::var("NUM_JOBS")
        .unwrap_or_else(|_| "1".to_string())
        .parse::<usize>()
        .expect("NUM_JOBS must be a positive integer");
    assert!(jobs > 0, "NUM_JOBS must be a positive integer");
    jobs.min(16).to_string()
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
