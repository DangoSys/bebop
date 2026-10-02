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
    println!("cargo:rustc-check-cfg=cfg(P2E_DIFF)");
    println!("cargo:rustc-check-cfg=cfg(P2E_ACCESS)");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build/link.rs");
    println!("cargo:rerun-if-changed=build/vsrc.rs");
    println!("cargo:rerun-if-changed=build/vvac.rs");
    println!("cargo:rerun-if-env-changed=VSRC_PATH");
    println!("cargo:rerun-if-env-changed=OUT_PATH");
    println!("cargo:rerun-if-env-changed=P2E_VERIFICATION_MODE");

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
    let trace_mode_path = out_dir.join("p2e_trace_mode");
    let mode = env::var("P2E_VERIFICATION_MODE").unwrap_or_else(|_| "none".to_string());
    assert!(
        ["none", "access", "difftest-n"].contains(&mode.as_str()),
        "invalid verification mode"
    );
    println!("cargo:rustc-env=P2E_VERIFICATION_MODE={mode}");
    let trace_mode = mode.as_str();
    if mode == "difftest-n" {
        println!("cargo:rustc-cfg=P2E_DIFF");
    }
    if mode == "access" {
        println!("cargo:rustc-cfg=P2E_ACCESS");
    }
    println!("cargo:rerun-if-changed={}", libctb_dst.display());
    println!("cargo:rerun-if-changed={}", trace_mode_path.display());

    println!("cargo:rustc-env=P2E_DUT_CONFIG_SHA256=unlinked");
    if libctb_dst.exists() {
        let fingerprint = std::fs::read_to_string(out_dir.join("dut-config.sha256")).expect("DUT fingerprint");
        println!("cargo:rustc-env=P2E_DUT_CONFIG_SHA256={fingerprint}");
        if let Ok(rtl) = env::var("VSRC_PATH") {
            assert_eq!(
                std::fs::read_to_string(PathBuf::from(&rtl).join("verification-mode")).unwrap(),
                mode,
                "RTL mode changed"
            );
            assert_eq!(
                std::fs::read_to_string(PathBuf::from(rtl).join("dut-config.sha256")).unwrap(),
                fingerprint,
                "DUT config changed; use a fresh OUT_PATH"
            );
        }
        let cached_mode =
            std::fs::read_to_string(&trace_mode_path).expect("missing P2E trace mode; rebuild in a fresh OUT_PATH");
        assert_eq!(
            cached_mode, trace_mode,
            "P2E trace mode changed; rebuild in a fresh OUT_PATH"
        );
        vvac::verify_trace(&out_dir, &mode);
        println!("cargo:warning=Found existing libvCtb.so, skipping VVAC build");
        println!("cargo:warning=Building C++ wrapper for Rust FFI...");
        link::build_cpp_wrapper(&manifest_dir, &out_dir, &mode);
        link::link_vvac(&libctb_dst);
        return;
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
    assert_eq!(
        std::fs::read_to_string(build_dir.join("verification-mode")).expect("RTL mode manifest"),
        mode,
        "RTL verification mode mismatch"
    );
    std::fs::create_dir_all(&out_dir).expect("create P2E output");
    std::fs::copy(build_dir.join("dut-config"), out_dir.join("dut-config")).expect("copy DUT configuration");
    std::fs::copy(build_dir.join("dut-config.sha256"), out_dir.join("dut-config.sha256"))
        .expect("copy DUT fingerprint");
    println!(
        "cargo:rustc-env=P2E_DUT_CONFIG_SHA256={}",
        std::fs::read_to_string(build_dir.join("dut-config.sha256")).unwrap()
    );

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
    vsrc::write_flist(&flist, &vsrcs, &mode);
    println!("cargo:warning=P2E trace mode: {trace_mode}");

    println!("cargo:warning=Removing empty module instantiations from Verilog...");
    vvac::remove_empty_module_instantiations(&build_dir);

    println!("cargo:warning=Running vvac (first pass) to generate empty module stubs...");
    vvac::run_vvac(&out_dir, &sourceme, &flist, P2E_TOP, &mode);

    println!("cargo:warning=Adding missing empty modules to VVAC filelist...");
    let needs_rebuild = vvac::add_missing_empty_modules(&out_dir);

    if needs_rebuild {
        println!("cargo:warning=Running vvac (second pass) with complete filelist...");
        vvac::run_vvac(&out_dir, &sourceme, &flist, P2E_TOP, &mode);
    }

    println!("cargo:warning=Copying libvCtb.so from vvac output...");
    let libctb_src = out_dir.join("vvacDir/runtimeDir/lib/lib_arm/libvCtb.so");
    let libctb_dst = out_dir.join("libvCtb.so");
    vsrc::assert_exists(&libctb_src, "libvCtb.so not found. vvac may have failed");
    std::fs::copy(&libctb_src, &libctb_dst).expect("copy libvCtb.so");
    std::fs::write(&trace_mode_path, trace_mode).expect("write P2E trace mode");
    println!(
        "cargo:warning=Copied libvCtb.so from {} to {}",
        libctb_src.display(),
        libctb_dst.display()
    );

    vvac::fix_library_rpath(&out_dir);

    println!("cargo:warning=Building C++ wrapper for Rust FFI...");
    link::build_cpp_wrapper(&manifest_dir, &out_dir, &mode);

    println!("cargo:warning=Linking vvac and C++ wrapper...");
    link::link_vvac(&libctb_dst);

    println!("cargo:warning=P2E VVAC build complete!");
}
