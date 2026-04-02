use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn get_make_jobs() -> String {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(16)
        .max(1)
        .to_string()
}

fn emit_arch_verilog(manifest: &Path) {
    println!("cargo:rerun-if-changed=scripts/emit-arch-cosim-verilog.sh");
    let gen_out = manifest.join("src/verilator/gen");
    let gen_be = gen_out.join("BebopBuckyballSubsystemCosim.sv");
    let gen_vec = gen_out.join("VecComputeTop.sv");
    let arch_root = manifest.join("../arch");
    let script = manifest.join("scripts/emit-arch-cosim-verilog.sh");
    if !script.is_file() {
        panic!("missing {}; cannot emit arch Verilog", script.display());
    }

    if !arch_root.is_dir() {
        if gen_be.is_file() && gen_vec.is_file() {
            return;
        }
        panic!(
            "arch checkout not found at {} and Verilog gen is incomplete (need {} and {}); place arch repo or run mill emit into {}",
            arch_root.display(),
            gen_be.display(),
            gen_vec.display(),
            gen_out.display()
        );
    }

    let st = Command::new("bash")
        .arg(script.as_os_str())
        .arg(gen_out.as_os_str())
        .arg(arch_root.as_os_str())
        .status()
        .unwrap_or_else(|e| panic!("spawn emit-arch-cosim-verilog.sh: {e}"));
    if !st.success() {
        panic!(
            "emit-arch-cosim-verilog.sh failed; need mill in PATH when emitting into {}",
            gen_out.display()
        );
    }
}

fn main() {
    println!("cargo:rerun-if-changed=src/verilator/bebop_accel.sv");
    println!("cargo:rerun-if-changed=src/verilator/bebop_cosim_banks.sv");
    println!("cargo:rerun-if-changed=src/verilator/cosim.cpp");
    println!("cargo:rerun-if-changed=src/verilator/gen/VecComputeTop.sv");
    println!("cargo:rerun-if-changed=src/verilator/gen/BebopBuckyballSubsystemCosim.sv");
    if env::var("CARGO_FEATURE_VERILATOR").is_err() {
        return;
    }
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    emit_arch_verilog(manifest.as_path());
    let vl_dir = out.join("vl_bebop");
    std::fs::create_dir_all(&vl_dir).expect("create vl_bebop");
    let gen_sv = manifest.join("src/verilator/gen/BebopBuckyballSubsystemCosim.sv");
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
    let jobs = get_make_jobs();
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
        .arg(format!("-j{jobs}"))
        .arg("libVbebop_accel")
        .status()
        .unwrap_or_else(|e| panic!("spawn make: {e}"));
    if !make_st.success() {
        panic!("make libVbebop_accel failed");
    }
    println!("cargo:rustc-link-search=native={}", vl_dir.display());
}
