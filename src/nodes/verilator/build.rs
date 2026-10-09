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
//===---------------------------------------------------------------------------===//
//
//
//
//===---------------------------------------------------------------------------===//

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const TOPNAME: &str = "BBSimHarness";

const VERILATOR_ARGS: &[&str] = &[
    "-MMD",
    "-cc",
    "--vpi",
    "--trace",
    "-O3",
    "-fno-dedup",
    "--x-assign",
    "fast",
    "--x-initial",
    "fast",
    "--noassert",
    "-Wno-fatal",
    "--trace-fst",
    "--output-split",
    "10000",
    "--output-split-cfuncs",
    "100",
    "--unroll-count",
    "256",
    "-Wall",
    "-Wno-PINCONNECTEMPTY",
    "-Wno-ASSIGNDLY",
    "-Wno-DECLFILENAME",
    "-Wno-UNUSED",
    "-Wno-UNUSEDSIGNAL",
    "-Wno-UNOPTFLAT",
    "-Wno-BLKANDNBLK",
    "-Wno-style",
    "--timing",
];

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let native_dir = manifest_dir.join("native");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let obj_dir = out_dir.join("obj_dir");

    let build_dir = resolve_vsrc_path();
    let jobs = requested_jobs();

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", build_dir.display());
    println!("cargo:rerun-if-env-changed=VSRC_PATH");
    println!("cargo:rerun-if-env-changed=RISCV");

    let riscv = require_nix_riscv();

    let vsrcs = collect_files(&build_dir, &["v", "sv"]);
    let mut csrcs = collect_build_csrcs(&build_dir);
    csrcs.extend([
        native_dir.join("verilator.cc"),
        native_dir.join("memory/BBSimDRAM.cc"),
        native_dir.join("memory/mm.cc"),
        native_dir.join("memory/mm_dramsim3.cc"),
    ]);
    let native_inputs = collect_files(&native_dir, &["c", "cc", "cpp", "h", "hh", "hpp"]);
    for src in vsrcs.iter().chain(csrcs.iter()).chain(native_inputs.iter()) {
        println!("cargo:rerun-if-changed={}", src.display());
    }

    fs::create_dir_all(&obj_dir).expect("create obj_dir");
    run_verilator(&build_dir, &obj_dir, TOPNAME, &jobs, &vsrcs, &csrcs, &native_dir, &riscv);
    // The generated makefile owns hierarchy libraries, source partitioning and runtime support.
    let status = Command::new("make")
        .arg("-s").arg("-C").arg(&obj_dir)
        .arg("-f").arg(format!("V{TOPNAME}.mk"))
        .arg("-j").arg(&jobs)
        .arg(format!("CXX={}", require_gxx()))
        .arg("OPT_FAST=-O3").arg("OPT_SLOW=-O3")
        .arg(format!("libV{TOPNAME}.a")).arg("libverilated.a")
        .status().expect("compile Verilator libraries");
    assert!(status.success(), "Verilator library compilation failed: {status}");
    emit_link_config(&obj_dir, &riscv);
}

struct NixRiscv {
    include_dir: PathBuf,
    lib_dir: PathBuf,
}

fn require_gxx() -> String {
    let cxx = "g++";
    let status = Command::new(cxx)
        .arg("--version")
        .stdout(Stdio::null())
        .status()
        .expect("g++ must be available in the nix development environment");
    assert!(
        status.success(),
        "g++ must be runnable in the nix development environment"
    );
    cxx.to_string()
}

fn require_nix_riscv() -> NixRiscv {
    let root = PathBuf::from(env::var("RISCV").expect("RISCV must be set by the nix development environment"));
    assert_exists(&root, "RISCV path does not exist");

    let include_dir = root.join("include");
    let lib_dir = root.join("lib");
    let dramsim3_config = root
        .join("share")
        .join("dramsim3")
        .join("configs")
        .join("DDR3_1Gb_x8_1333.ini");
    let required_files = vec![
        include_dir.join("dramsim3.h"),
        lib_dir.join("libdramsim3.so"),
        lib_dir.join("libz.so"),
        dramsim3_config.clone(),
    ];
    for path in &required_files {
        assert_exists(path, "nix RISCV dependency is missing");
    }

    NixRiscv { include_dir, lib_dir }
}

fn emit_link_config(native_lib_dir: &Path, riscv: &NixRiscv) {
    println!("cargo:rustc-link-search=native={}", native_lib_dir.display());
    println!("cargo:rustc-link-search=native={}", riscv.lib_dir.display());
    println!("cargo:rustc-link-lib=static=V{TOPNAME}");
    println!("cargo:rustc-link-lib=static=verilated");
    println!("cargo:rustc-link-lib=stdc++");
    println!("cargo:rustc-link-lib=dylib=dramsim3");
    println!("cargo:rustc-link-lib=lz4");
    println!("cargo:rustc-link-lib=z");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", riscv.lib_dir.display());
}

fn resolve_vsrc_path() -> PathBuf {
    let path = env::var("VSRC_PATH").expect("VSRC_PATH must be set before building bebop-verilator");
    assert!(!path.trim().is_empty(), "VSRC_PATH must not be empty");
    let path = PathBuf::from(path);
    assert_exists(&path, "VSRC_PATH does not point to a Verilog source directory");
    path
}

fn requested_jobs() -> String {
    let jobs = env::var("NUM_JOBS")
        .unwrap_or_else(|_| "1".to_string())
        .parse::<usize>()
        .expect("NUM_JOBS must be a positive integer");
    assert!(jobs > 0, "NUM_JOBS must be a positive integer");
    jobs.to_string()
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
    let entries = fs::read_dir(root).unwrap_or_else(|e| panic!("read directory {} failed: {e}", root.display()));
    for entry in entries {
        let path = entry
            .unwrap_or_else(|e| panic!("read directory entry under {} failed: {e}", root.display()))
            .path();
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

fn run_verilator(build_dir: &Path, obj_dir: &Path, topname: &str, jobs: &str, vsrcs: &[PathBuf], csrcs: &[PathBuf], native_dir: &Path, riscv: &NixRiscv) {
    let mut cmd = Command::new("verilator");
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());

    for arg in VERILATOR_ARGS {
        cmd.arg(arg);
    }

    cmd.arg("-j")
        .arg(jobs)
        .arg(format!("+incdir+{}", build_dir.display()))
        .arg("--top-module")
        .arg(topname)
        .arg("--Mdir")
        .arg(obj_dir);

    let hierarchy = build_dir.join("hierarchy.vlt");
    if hierarchy.is_file() {
        cmd.arg("--hierarchical").arg(hierarchy);
    }
    cmd.arg("-CFLAGS").arg("-std=c++17 -fcoroutines -pthread");
    for directory in [native_dir.to_path_buf(), native_dir.join("include"), riscv.include_dir.clone()] {
        cmd.arg("-CFLAGS").arg(format!("-I{}", directory.display()));
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
