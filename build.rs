use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/verilator/bebop_accel.sv");
    println!("cargo:rerun-if-changed=src/verilator/bebop_cosim_banks.sv");
    println!("cargo:rerun-if-changed=src/verilator/cosim.cpp");
    println!("cargo:rerun-if-changed=src/verilator/gen/VecComputeTop.sv");
    if env::var("CARGO_FEATURE_VERILATOR").is_err() {
        return;
    }
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let vl_dir = out.join("vl_bebop");
    let _ = std::fs::remove_dir_all(&vl_dir);
    std::fs::create_dir_all(&vl_dir).expect("create vl_bebop");
    let gen_sv = manifest.join("src/verilator/gen/BebopSpikeCosimTop.sv");
    let vec_sv = manifest.join("src/verilator/gen/VecComputeTop.sv");
    let sv = manifest.join("src/verilator/bebop_accel.sv");
    let cosim = manifest.join("src/verilator/cosim.cpp");
    if !gen_sv.is_file() {
        panic!(
            "missing {}; run arch: mill buckyball.runMain sims.bebop.EmitBebopSpikeCosimVerilog <bebop>/src/verilator/gen (see scripts/emit-arch-cosim-verilog.sh)",
            gen_sv.display()
        );
    }
    if !vec_sv.is_file() {
        panic!(
            "missing {}; run arch: mill buckyball.runMain sims.bebop.EmitBebopSpikeCosimVerilog <bebop>/src/verilator/gen (see scripts/emit-arch-cosim-verilog.sh)",
            vec_sv.display()
        );
    }
    println!("cargo:rerun-if-changed={}", gen_sv.display());
    println!("cargo:rerun-if-changed={}", vec_sv.display());
    let st = Command::new("verilator")
        .args([
            "--cc",
            "-Mdir",
            vl_dir.to_str().expect("utf8 vl_dir"),
            "--top-module",
            "bebop_accel",
            "-Wno-TIMESCALEMOD",
            "-CFLAGS",
            "-fPIC -O2",
            gen_sv.to_str().expect("utf8 gen_sv"),
            vec_sv.to_str().expect("utf8 vec_sv"),
            manifest
                .join("src/verilator/bebop_cosim_banks.sv")
                .to_str()
                .expect("utf8 banks sv"),
            sv.to_str().expect("utf8 sv"),
            cosim.to_str().expect("utf8 cosim"),
        ])
        .status()
        .unwrap_or_else(|e| panic!("spawn verilator: {e}"));
    if !st.success() {
        panic!("verilator failed; install Verilator and ensure it is in PATH");
    }
    let make_st = Command::new("make")
        .current_dir(&vl_dir)
        .arg("-f")
        .arg("Vbebop_accel.mk")
        .arg("libVbebop_accel")
        .env("CXX", env::var("CXX").unwrap_or_else(|_| "c++".to_string()))
        .status()
        .unwrap_or_else(|e| panic!("spawn make: {e}"));
    if !make_st.success() {
        panic!("make libVbebop_accel failed");
    }
    println!("cargo:rustc-link-search=native={}", vl_dir.display());
}
