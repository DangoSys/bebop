#[path = "../../../../../../bebop/src/nodes/bemu/build_support/mod.rs"]
mod build_support;

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let p2e_abi = env::var_os("BEBOP_BEMU_P2E_ABI").is_some();
    let dispatch = manifest_dir.join("dispatch.rs");
    let pb = manifest_dir
        .parent()
        .expect("bemu manifest must live under examples/chips/<chip>/configs/generated/bemu")
        .join("chip.pb");
    let proto = manifest_dir.join("../../../../../../bbdev/api/steps/config/scripts/proto/chip.proto");
    if !dispatch.is_file() {
        panic!("missing {}; run bbdev config --install", dispatch.display());
    }
    if !pb.is_file() {
        panic!("missing {}; run bbdev config --install", pb.display());
    }
    if !proto.is_file() {
        panic!("missing {}", proto.display());
    }
    fs::copy(&dispatch, out_dir.join("chip_balls.rs")).expect("copy dispatch.rs");
    let proto_dir = proto.parent().expect("chip.proto parent").to_path_buf();
    prost_build::compile_protos(&[&proto], &[&proto_dir]).unwrap_or_else(|e| panic!("prost: {e}"));
    println!("cargo:rerun-if-changed={}", dispatch.display());
    println!("cargo:rerun-if-changed={}", pb.display());
    println!("cargo:rerun-if-changed={}", proto.display());

    if p2e_abi {
        let cc = env::var("BEBOP_BEMU_CC").expect("BEBOP_BEMU_CC");
        env::set_var("CC", &cc);
        env::set_var("CXX", env::var("BEBOP_BEMU_CXX").expect("BEBOP_BEMU_CXX"));
        env::set_var("DTC", env::var("BEBOP_BEMU_DTC").expect("BEBOP_BEMU_DTC"));
        env::set_var(
            "PATH",
            format!(
                "{}:/usr/bin:/bin",
                PathBuf::from(cc).parent().expect("BEBOP_BEMU_CC parent").display()
            ),
        );
        env::set_var(
            "LD_LIBRARY_PATH",
            env::var("BEBOP_BEMU_COMPILER_LIBRARY_PATH").expect("BEBOP_BEMU_COMPILER_LIBRARY_PATH"),
        );
        for name in [
            "NIX_CFLAGS_COMPILE",
            "NIX_CFLAGS_COMPILE_FOR_TARGET",
            "NIX_LDFLAGS",
            "NIX_LDFLAGS_FOR_TARGET",
            "CPATH",
            "LIBRARY_PATH",
            "C_INCLUDE_PATH",
            "CPLUS_INCLUDE_PATH",
            "CFLAGS",
            "CXXFLAGS",
            "LDFLAGS",
        ] {
            env::remove_var(name);
        }
    }

    let engine = manifest_dir.join("../../../../../../bebop/src/nodes/bemu");
    let native_dir = build_support::spike::native_dir(&engine);
    let spike_dir = native_dir.join("spike");
    let spike_install_dir = out_dir.join(if p2e_abi { "spike_install_p2e" } else { "spike_install" });
    let spike_build_dir = out_dir.join(if p2e_abi { "spike_build_p2e" } else { "spike_build" });
    build_support::spike::build_and_link(&native_dir, &spike_dir, &spike_build_dir, &spike_install_dir);
    println!(
        "cargo:rustc-env=BEBOP_BEMU_SPIKE_LIB_DIR={}",
        spike_install_dir.join("lib").display()
    );
    build_support::rerun::emit_engine(&engine, &native_dir);
}
