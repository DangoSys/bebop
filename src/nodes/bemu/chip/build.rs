#[path = "../../../../../../bebop/src/nodes/bemu/build_support/mod.rs"]
mod build_support;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn configure_p2e_toolchain(manifest_dir: &Path) {
    let root = Path::new("/home/x-epic/hpe-24.12.01.s008/tools/gcc-8.3.0");
    let bin = root.join("gcc-8.3.0/bin");
    env::set_var("CC", bin.join("gcc"));
    env::set_var("CXX", bin.join("g++"));
    env::set_var("DTC", manifest_dir.join("../../../../../../result/bin/dtc"));
    env::set_var("PATH", format!("{}:/usr/bin:/bin", bin.display()));
    env::set_var(
        "LD_LIBRARY_PATH",
        ["gmp-6.2.1/lib", "mpfr-4.1.0/lib", "mpc-1.2.1/lib"]
            .map(|path| root.join(path))
            .iter()
            .map(|path| path.to_string_lossy())
            .collect::<Vec<_>>()
            .join(":"),
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

fn install_runtime_libraries(install_dir: &Path) {
    let target_dir = PathBuf::from(env::var("CARGO_TARGET_DIR").expect("CARGO_TARGET_DIR"));
    let profile = env::var("PROFILE").expect("PROFILE");
    let destination = target_dir.join(profile).join("bemu-runtime");
    fs::create_dir_all(&destination).expect("create BEMU runtime directory");
    for entry in fs::read_dir(install_dir.join("lib")).expect("read Spike install lib") {
        let entry = entry.expect("read Spike runtime library entry");
        let name = entry.file_name();
        if name.to_string_lossy().contains(".so") {
            fs::copy(entry.path(), destination.join(name)).expect("install BEMU runtime library");
        }
    }
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
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

    let p2e = env::var_os("CARGO_FEATURE_P2E").is_some();
    if p2e {
        configure_p2e_toolchain(&manifest_dir);
    }

    let engine = manifest_dir.join("../../../../../../bebop/src/nodes/bemu");
    let native_dir = build_support::spike::native_dir(&engine);
    let spike_dir = native_dir.join("spike");
    let suffix = if p2e { "_p2e" } else { "" };
    let spike_install_dir = out_dir.join(format!("spike_install{suffix}"));
    let spike_build_dir = out_dir.join(format!("spike_build{suffix}"));
    build_support::spike::build_and_link(&native_dir, &spike_dir, &spike_build_dir, &spike_install_dir);
    install_runtime_libraries(&spike_install_dir);
    println!(
        "cargo:rustc-env=BEBOP_BEMU_SPIKE_LIB_DIR={}",
        spike_install_dir.join("lib").display()
    );
    build_support::rerun::emit_engine(&engine, &native_dir);
}
