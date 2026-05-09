//===- build.rs - Build BEMU for buckyball simulation ---------------------===//
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
//===----------------------------------------------------------------------===//
//
// 1. Why build Spike first?
//  Spike headers/libs are required for bemu's CPU part simulation.
//
// 2. How to build bemu?
//  Link Spike libs with rpath, let bemu can find the libraries at runtime.
//  Spike API calls come from riscv/{processor,extension,rocc}.h in libriscv
//
//===----------------------------------------------------------------------===//

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let native_dir = manifest_dir.join("native");
    let spike_dir = native_dir.join("spike");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let spike_install_dir = out_dir.join("spike_install");
    let spike_build_dir = out_dir.join("spike_build");

    if !spike_dir.exists() || !spike_dir.join("configure.ac").exists() {
        panic!("Spike missing at {}.", spike_dir.display());
    }

    // Incremental compilation check
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", native_dir.join("rocc.cc").display());
    println!("cargo:rerun-if-changed={}", native_dir.join("spike.cc").display());

    // Build and install spike
    build_spike(&spike_dir, &spike_build_dir, &spike_install_dir);

    // Compile spike.cc and rocc.cc together
    cc::Build::new()
        .cpp(true)
        .file(native_dir.join("spike.cc"))
        .file(native_dir.join("rocc.cc"))
        .include(spike_install_dir.join("include/riscv"))
        .include(spike_install_dir.join("include/fesvr"))
        .flag("-std=c++17")
        .compile("spike_wrapper");

    println!("cargo:rustc-link-search=native={}/lib", spike_install_dir.display());
    println!("cargo:rustc-link-lib=dylib=riscv");
    println!("cargo:rustc-link-lib=dylib=disasm");
    println!("cargo:rustc-link-lib=dylib=softfloat");
    println!("cargo:rustc-link-lib=dylib=fesvr");
    println!("cargo:rustc-link-lib=dylib=stdc++");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}/lib", spike_install_dir.display());
}

fn spike_configure(spike_dir: &Path, build_dir: &Path, install_dir: &Path) {
    let st = Command::new(spike_dir.join("configure"))
        .current_dir(build_dir)
        .arg("--prefix")
        .arg(install_dir)
        .args([
            "--with-boost=no",
            "--with-boost-asio=no",
            "--with-boost-regex=no",
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("failed to execute configure");

    if !st.success() {
        panic!("spike configure failed with status {}", st);
    }
}

fn spike_make(build_dir: &Path) {
    let st = Command::new("make")
        .current_dir(build_dir)
        .arg("-j")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("run spike make");

    if !st.success() {
        panic!("spike make failed with status {}", st);
    }
}

fn spike_make_install(build_dir: &Path) {
    let st = Command::new("make")
        .current_dir(build_dir)
        .arg("install")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("run spike make install");

    if !st.success() {
        panic!("spike make install failed with status {}", st);
    }
}

fn build_spike(spike_dir: &Path, build_dir: &Path, install_dir: &Path) {
    fs::create_dir_all(build_dir).expect("create spike build dir");
    fs::create_dir_all(install_dir).expect("create spike install dir");

    println!("cargo:warning=Building spike from {}", spike_dir.display());

    spike_configure(spike_dir, build_dir, install_dir);
    spike_make(build_dir);
    spike_make_install(build_dir);

    println!("cargo:warning=Spike installed to {}", install_dir.display());
}
