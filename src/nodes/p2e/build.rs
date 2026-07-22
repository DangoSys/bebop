//===-------- build.rs - Build P2E simulation workflow --------------------===//
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
//===---------------------------------------------------------------------===//

#[path = "build/link.rs"]
mod link;
#[path = "build/vsrc.rs"]
mod vsrc;
#[path = "build/vvac.rs"]
mod vvac;

use std::env;
use std::path::PathBuf;

const P2E_TOP: &str = "P2ETop";
const SOURCE_ME: &str = "sourceme.sh";

fn main() {
    println!("cargo:rustc-check-cfg=cfg(vvac_linked)");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=VSRC_PATH");
    println!("cargo:rerun-if-env-changed=OUT_PATH");
    println!("cargo:rerun-if-env-changed=BEBOP_P2E_REBUILD_RUNTIME");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let bebop_root = manifest_dir
        .ancestors()
        .nth(3)
        .expect("p2e crate should live under bebop/src/nodes/p2e")
        .to_path_buf();

    let out_dir = match env::var("OUT_PATH") {
        Ok(path) => PathBuf::from(path),
        Err(_) => bebop_root.join("out"),
    };
    let libctb_dst = out_dir.join("libvCtb.so");
    let rebuild_runtime = env::var_os("BEBOP_P2E_REBUILD_RUNTIME").is_some();

    if libctb_dst.exists() && !rebuild_runtime {
        println!("cargo:warning=Found existing libvCtb.so, skipping VVAC build");
        println!("cargo:warning=Building C++ wrapper for Rust FFI...");
        link::build_cpp_wrapper(&manifest_dir, &out_dir);
        link::link_vvac(&libctb_dst);
        return;
    }

    if rebuild_runtime {
        println!("cargo:warning=Forcing VVAC runtime rebuild");
    }

    let vsrc_path = match env::var("VSRC_PATH") {
        Ok(path) => path,
        Err(_) => {
            println!("cargo:warning=VSRC_PATH not set and libvCtb.so not found");
            println!("cargo:warning=To build P2E, set VSRC_PATH to your Verilog source directory");
            println!("cargo:warning=Example: VSRC_PATH=/home/wanghui/Code/buckyball/arch/build/sims.p2e.P2EToyConfig");
            return;
        }
    };
    let build_dir = PathBuf::from(&vsrc_path);

    let sourceme = manifest_dir.join(SOURCE_ME);
    vsrc::assert_exists(&sourceme, "missing p2e sourceme script");

    println!("cargo:warning=Cleaning old build artifacts...");
    vsrc::clean_build_artifacts(&out_dir);

    vsrc::assert_exists(
        &build_dir,
        &format!("missing Verilog source directory at VSRC_PATH={}", vsrc_path),
    );
    vsrc::assert_exists(
        &build_dir.join(format!("{P2E_TOP}.sv")),
        &format!("VSRC_PATH={} does not look like a P2E build", vsrc_path),
    );
    let mut vsrcs = vsrc::collect_files(&build_dir, &["v", "sv"]);

    let ddr4_stub = manifest_dir.join("src/ddr/ip/xepic_ddr4_dc1_stub.sv");
    vsrc::assert_exists(&ddr4_stub, "xepic_ddr4_dc1 stub not found");
    vsrcs.push(ddr4_stub);
    println!("cargo:warning=Added xepic_ddr4_dc1 stub from src/ddr/ip");
    println!("cargo:rerun-if-changed={}", build_dir.display());

    std::fs::create_dir_all(&out_dir).expect("create p2e out directory");
    let flist = out_dir.join("p2e_vvac_filelist.f");
    vsrc::write_flist(&flist, &vsrcs);

    println!("cargo:warning=Removing empty module instantiations from Verilog...");
    vvac::remove_empty_module_instantiations(&build_dir);

    println!("cargo:warning=Running vvac (first pass) to generate empty module stubs...");
    vvac::run_vvac(&out_dir, &sourceme, &flist, P2E_TOP);

    println!("cargo:warning=Adding missing empty modules to VVAC filelist...");
    let needs_rebuild = vvac::add_missing_empty_modules(&out_dir);

    if needs_rebuild {
        println!("cargo:warning=Running vvac (second pass) with complete filelist...");
        vvac::run_vvac(&out_dir, &sourceme, &flist, P2E_TOP);
    }

    println!("cargo:warning=Copying libvCtb.so from vvac output...");
    let libctb_src = out_dir.join("vvacDir/runtimeDir/lib/lib_arm/libvCtb.so");
    let libctb_dst = out_dir.join("libvCtb.so");
    vsrc::assert_exists(&libctb_src, "libvCtb.so not found. vvac may have failed");
    std::fs::copy(&libctb_src, &libctb_dst).expect("copy libvCtb.so");
    println!(
        "cargo:warning=Copied libvCtb.so from {} to {}",
        libctb_src.display(),
        libctb_dst.display()
    );

    println!("cargo:warning=Fixing VVAC library RPATH for C++ ABI compatibility...");
    vvac::fix_vvac_library_rpath(&out_dir);

    println!("cargo:warning=Building C++ wrapper for Rust FFI...");
    link::build_cpp_wrapper(&manifest_dir, &out_dir);

    println!("cargo:warning=Linking vvac and C++ wrapper...");
    link::link_vvac(&libctb_dst);

    println!("cargo:warning=P2E VVAC build complete!");
}
