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
//===-----------------------------------------------------------------===//-----===//
//
// 1. Why build Spike first?
//  Spike headers/libs are required for bemu's CPU part simulation.
//
// 2. How to build bemu?
//  Link Spike libs with rpath, let bemu can find the libraries at runtime.
//  Spike API calls come from riscv/{processor,extension,rocc}.h in libriscv
//
// 3. How to register instructions?
//  Manually register in INSTRUCTIONS array, build.rs generates dispatch code.
//
//===-----------------------------------------------------------------===//-----===//

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug)]
struct MemoryModel {
    bank_num: usize,
    bank_width_bits: usize,
    bank_entries: usize,
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let native_dir = native_dir(&manifest_dir);
    let spike_dir = native_dir.join("spike");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let chip_inst = chip_inst(&manifest_dir);
    fs::write(
        out_dir.join("chip.rs"),
        format!("#[path = \"{}\"]\npub mod active_chip;\n", chip_inst.display()),
    )
    .expect("write active chip module");
    let memory_model = memory_model(&chip_inst);
    fs::write(out_dir.join("memory_model.rs"), render_memory_model(memory_model))
        .expect("write active chip memory model");

    let spike_install_dir = out_dir.join("spike_install");
    let spike_build_dir = out_dir.join("spike_build");

    if !spike_dir.exists() || !spike_dir.join("configure.ac").exists() {
        panic!("Spike missing at {}.", spike_dir.display());
    }

    // Incremental compilation check
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=BEBOP_BEMU_CHIP_INST");
    println!("cargo:rerun-if-env-changed=BEBOP_BEMU_NATIVE_DIR");
    println!("cargo:rerun-if-changed={}", chip_inst.display());
    println!("cargo:rerun-if-changed={}", native_dir.join("rocc.cc").display());
    println!("cargo:rerun-if-changed={}", native_dir.join("spike.cc").display());
    println!("cargo:rerun-if-changed={}", native_dir.join("btif.cc").display());
    println!("cargo:rerun-if-changed={}", native_dir.join("btif.h").display());

    // Build and install spike
    build_spike(&spike_dir, &spike_build_dir, &spike_install_dir);

    cc::Build::new()
        .cpp(true)
        .file(native_dir.join("spike.cc"))
        .file(native_dir.join("rocc.cc"))
        .file(native_dir.join("btif.cc"))
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
    // Set rpath so the binary can find the libraries at bebop's runtime
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}/lib", spike_install_dir.display());
}

/// Load the physical private-Bank layout from the active chip's architecture
/// configuration. BEMU is compiled with one active chip instruction set, so
/// generating constants here keeps its memory model tied to that same chip.
fn memory_model(chip_inst: &Path) -> MemoryModel {
    let config = chip_inst.ancestors().find_map(|ancestor| {
        let candidate = ancestor.join("arch/src/main/scala/configs/tiles/cores/memdomains/default.toml");
        candidate.is_file().then_some(candidate)
    });
    let Some(config) = config else {
        // The built-in base instruction set has no chip architecture tree.
        // Preserve the historical base-model layout for that standalone mode.
        return MemoryModel {
            bank_num: 32,
            bank_width_bits: 128,
            bank_entries: 128,
        };
    };

    println!("cargo:warning=BEMU memory model: {}", config.display());
    println!("cargo:rerun-if-changed={}", config.display());
    let text = fs::read_to_string(&config)
        .unwrap_or_else(|error| panic!("read BEMU memory model {}: {error}", config.display()));
    let bank_num = toml_integer(&text, "bank", "num", &config);
    let bank_width_bits = toml_integer(&text, "bank", "width", &config);
    let bank_entries = toml_integer(&text, "bank", "entries", &config);
    assert!(bank_num > 0, "BEMU memory model {} has bank.num=0", config.display());
    assert!(
        bank_width_bits > 0 && bank_width_bits.is_multiple_of(8),
        "BEMU memory model {} has invalid bank.width={bank_width_bits}",
        config.display()
    );
    assert!(
        bank_entries > 0,
        "BEMU memory model {} has bank.entries=0",
        config.display()
    );

    MemoryModel {
        bank_num,
        bank_width_bits,
        bank_entries,
    }
}

fn toml_integer(text: &str, wanted_section: &str, wanted_key: &str, path: &Path) -> usize {
    let mut section = "";
    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        if section != wanted_section {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != wanted_key {
            continue;
        }
        return value
            .trim()
            .replace('_', "")
            .parse()
            .unwrap_or_else(|error| panic!("parse [{wanted_section}].{wanted_key} in {}: {error}", path.display()));
    }
    panic!("missing [{wanted_section}].{wanted_key} in {}", path.display());
}

fn render_memory_model(model: MemoryModel) -> String {
    let mut generated = String::new();
    writeln!(generated, "// Generated by build.rs from the active chip memory model.").unwrap();
    writeln!(generated, "pub const BANK_NUM: usize = {};", model.bank_num).unwrap();
    writeln!(generated, "pub const BANK_WIDTH: usize = {};", model.bank_width_bits).unwrap();
    writeln!(generated, "pub const BANK_LINES: usize = {};", model.bank_entries).unwrap();
    writeln!(generated, "pub const BANK_SIZE: usize = BANK_LINES * (BANK_WIDTH / 8);").unwrap();
    generated
}

fn chip_inst(manifest_dir: &Path) -> PathBuf {
    if let Ok(path) = env::var("BEBOP_BEMU_CHIP_INST") {
        return PathBuf::from(path);
    }

    if manifest_dir.join("native").exists() {
        return manifest_dir.join("src/emu/inst/base.rs");
    }

    let chip = manifest_dir.join("src/lib.rs");
    if chip.exists() {
        return chip;
    }

    panic!("BEMU chip instruction set not found from {}.", manifest_dir.display());
}

fn native_dir(manifest_dir: &Path) -> PathBuf {
    if let Ok(dir) = env::var("BEBOP_BEMU_NATIVE_DIR") {
        return PathBuf::from(dir);
    }

    let local = manifest_dir.join("native");
    if local.join("spike").exists() {
        return local;
    }

    let repo_core = manifest_dir.join("../../../../bebop/src/nodes/bemu/native");
    if repo_core.join("spike").exists() {
        return repo_core;
    }

    panic!("BEMU native dir not found from {}.", manifest_dir.display());
}

fn spike_configure(spike_dir: &Path, build_dir: &Path, install_dir: &Path) {
    let st = Command::new(spike_dir.join("configure"))
        .current_dir(build_dir)
        .arg("--prefix")
        .arg(install_dir)
        .args(["--with-boost=no", "--with-boost-asio=no", "--with-boost-regex=no"])
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
