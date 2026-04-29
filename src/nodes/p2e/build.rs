use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const P2E_TOP: &str = "P2EHarness";
const SOURCE_ME: &str = "sourceme.sh";

fn main() {
    println!("cargo:rustc-check-cfg=cfg(vvac_linked)");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=ARCH_CONFIG");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let bebop_root = manifest_dir
        .ancestors()
        .nth(3)
        .expect("p2e crate should live under bebop/src/nodes/p2e")
        .to_path_buf();
    let buckyball_root = bebop_root
        .parent()
        .expect("bebop repo should live under buckyball root")
        .to_path_buf();
    let arch_dir = buckyball_root.join("arch");
    let out_dir = bebop_root.join("out");

    let config = env::var("ARCH_CONFIG").expect(
        "ARCH_CONFIG environment variable is required. Example: ARCH_CONFIG=sims.p2e.P2EToyConfig",
    );
    let build_dir = arch_dir.join("build").join(&config);
    assert_exists(
        &build_dir,
        &format!(
            "missing arch build directory for ARCH_CONFIG={}; generate Verilog first",
            config
        ),
    );
    assert_exists(
        &build_dir.join(format!("{P2E_TOP}.sv")),
        &format!("ARCH_CONFIG={} does not look like a P2E build", config),
    );

    let vsrcs = collect_files(&build_dir, &["v", "sv"]);
    assert!(
        !vsrcs.is_empty(),
        "no Verilog/SystemVerilog files found under {}",
        build_dir.display()
    );
    println!("cargo:rerun-if-changed={}", build_dir.display());

    fs::create_dir_all(&out_dir).expect("create p2e out directory");
    let flist = out_dir.join("p2e_vvac_filelist.f");
    write_flist(&flist, &vsrcs);
    let sourceme = manifest_dir.join(SOURCE_ME);
    assert_exists(&sourceme, "missing p2e sourceme script");

    println!("cargo:warning=Running vvac to generate DPI-C code...");
    run_vvac(&out_dir, &sourceme, &flist, P2E_TOP);

    println!("cargo:warning=Patching generated code for compatibility...");
    patch_generated_code(&out_dir);

    println!("cargo:warning=Building DPI-C library with cmake...");
    build_dpic_library(&out_dir);

    let libctb = out_dir.join("libvCtb.so");
    assert_exists(&libctb, "missing p2e libvCtb.so");

    println!("cargo:warning=Building C++ wrapper for Rust FFI...");
    build_cpp_wrapper(&manifest_dir, &out_dir);

    link_vvac(&libctb);

    println!("cargo:warning=✓ P2E VVAC build complete!");
}

fn run_vvac(out_dir: &Path, sourceme: &Path, flist: &Path, top: &str) {
    let bebop_root = out_dir.parent().expect("out_dir parent");

    // Run vvac through nix develop to get the right environment
    // vvac will fail at the cmake step due to LD_LIBRARY_PATH pollution, but that's OK
    // We'll manually compile the DPI-C library afterwards
    let script = format!(
        "cd '{}' && nix develop -c bash -c 'cd {} && unset LD_LIBRARY_PATH && mkdir -p {}/.dummy_bin && echo \"#!/bin/bash\" > {}/.dummy_bin/clang-format && echo \"exit 0\" >> {}/.dummy_bin/clang-format && chmod +x {}/.dummy_bin/clang-format && export PATH={}/.dummy_bin:$PATH && source {} && vvac -bc -f {} -top {} 2>&1' | tee {}",
        sh_quote(&bebop_root.display().to_string()),
        sh_quote(&out_dir.display().to_string()),
        sh_quote(&out_dir.display().to_string()),
        sh_quote(&out_dir.display().to_string()),
        sh_quote(&out_dir.display().to_string()),
        sh_quote(&out_dir.display().to_string()),
        sh_quote(&out_dir.display().to_string()),
        sh_quote(&sourceme.display().to_string()),
        sh_quote(&flist.display().to_string()),
        sh_quote(top),
        sh_quote(&out_dir.join("vvac_build.log").display().to_string()),
    );

    let status = Command::new("bash")
        .arg("-c")
        .arg(&script)
        .status()
        .expect("failed to execute vvac");

    // vvac will fail at cmake step, but that's expected - check if code was generated
    let dpic_dir = out_dir.join("vvac.tmp/dpic");
    assert!(
        dpic_dir.exists(),
        "vvac failed to generate dpic directory. Check log: {}",
        out_dir.join("vvac_build.log").display()
    );

    if !status.success() {
        println!("cargo:warning=vvac cmake step failed (expected), continuing with manual build...");
    }
}

fn patch_generated_code(out_dir: &Path) {
    // Patch CMakeLists.txt for cmake 4.x compatibility
    let cmake_lists = out_dir.join("vvac.tmp/dpic/CMakeLists.txt");
    if cmake_lists.exists() {
        let content = fs::read_to_string(&cmake_lists).expect("read CMakeLists.txt");
        let patched = content
            .replace(
                "cmake_minimum_required(VERSION 3.4.3)",
                "cmake_minimum_required(VERSION 3.5)"
            )
            .replace(
                "cmake_path(NORMAL_PATH LD OUTPUT_VARIABLE LD_OUT)",
                "set(LD_OUT \"${LD}\")  # cmake_path requires cmake 3.20+"
            );
        if content != patched {
            fs::write(&cmake_lists, patched).expect("write patched CMakeLists.txt");
        }
    }

    // Patch stub.h to fix vvac code generation bug (missing type for parameter)
    let stub_h = out_dir.join("vvac.tmp/dpic/ctb_gen/stub.h");
    if stub_h.exists() {
        let content = fs::read_to_string(&stub_h).expect("read stub.h");
        let patched = content.replace(
            "p2e_uart_write(uint32_t i0,  i1);",
            "p2e_uart_write(uint32_t i0, uint32_t i1);"
        );
        if content != patched {
            fs::write(&stub_h, patched).expect("write patched stub.h");
        }
    }
}

fn build_dpic_library(out_dir: &Path) {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sourceme = manifest_dir.join(SOURCE_ME);
    let dpic_dir = out_dir.join("vvac.tmp/dpic");
    let build_dir = dpic_dir.join("build");

    fs::create_dir_all(&build_dir).expect("create build directory");

    // Run cmake in clean environment (unset LD_LIBRARY_PATH to avoid HPE toolchain pollution)
    // But source sourceme.sh first to set VVAC_HOME and other required environment variables
    let bebop_root = out_dir.parent().expect("out_dir parent");
    let cmake_status = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "cd {} && nix develop -c bash -c 'cd {} && unset LD_LIBRARY_PATH && source {} && cmake .. -D_ARM_=ON -DCMAKE_BUILD_TYPE=Release'",
            sh_quote(&bebop_root.display().to_string()),
            sh_quote(&build_dir.display().to_string()),
            sh_quote(&sourceme.display().to_string()),
        ))
        .status()
        .expect("failed to run cmake");

    assert!(cmake_status.success(), "cmake configuration failed");

    // Run make
    let make_status = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "cd {} && nix develop -c bash -c 'cd {} && make -j8 && make install'",
            sh_quote(&bebop_root.display().to_string()),
            sh_quote(&build_dir.display().to_string()),
        ))
        .status()
        .expect("failed to run make");

    assert!(make_status.success(), "make build failed");

    // Copy libvCtb.so to expected location
    let src = dpic_dir.join("extern/lib/libvCtb.so");
    let dst = out_dir.join("libvCtb.so");
    assert!(src.exists(), "libvCtb.so not found at {}", src.display());
    fs::copy(&src, &dst).expect("copy libvCtb.so");
}

fn sh_quote(value: impl AsRef<str>) -> String {
    value.as_ref().replace('\'', "'\\''")
}

fn write_flist(path: &Path, sources: &[PathBuf]) {
    let mut contents = String::new();
    for src in sources {
        contents.push_str(&src.display().to_string());
        contents.push('\n');
    }
    fs::write(path, contents).expect("write p2e vvac filelist");
}

fn build_cpp_wrapper(manifest_dir: &Path, out_dir: &Path) {
    let wrapper_src = manifest_dir.join("src/ctb_wrapper.cpp");
    let wrapper_obj = out_dir.join("ctb_wrapper.o");
    let wrapper_lib = out_dir.join("libctb_wrapper.a");

    println!("cargo:rerun-if-changed={}", wrapper_src.display());

    // Compile C++ wrapper to object file
    let status = Command::new("g++")
        .args(&[
            "-c",
            "-fPIC",
            "-std=c++11",
            "-I", &out_dir.join("vvac.tmp/dpic/extern/include").display().to_string(),
            &wrapper_src.display().to_string(),
            "-o", &wrapper_obj.display().to_string(),
        ])
        .status()
        .expect("failed to compile C++ wrapper");

    assert!(status.success(), "C++ wrapper compilation failed");

    // Create static library from object file
    let status = Command::new("ar")
        .args(&[
            "rcs",
            &wrapper_lib.display().to_string(),
            &wrapper_obj.display().to_string(),
        ])
        .status()
        .expect("failed to create wrapper library");

    assert!(status.success(), "wrapper library creation failed");

    // Link the wrapper library
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=ctb_wrapper");
    println!("cargo:rustc-link-lib=dylib=stdc++");
}

fn link_vvac(libctb: &Path) {
    let lib_dir = libctb.parent().expect("libvCtb.so parent directory");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=vCtb");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    println!("cargo:rustc-cfg=vvac_linked");
}

fn assert_exists(path: &Path, message: &str) {
    assert!(path.exists(), "{message}: {}", path.display());
}

fn collect_files(root: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_inner(root, exts, &mut files);
    files.sort();
    files
}

fn collect_files_inner(root: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(root).unwrap_or_else(|_| panic!("read {}", root.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_inner(&path, exts, out);
            continue;
        }
        let Some(ext) = path.extension().and_then(OsStr::to_str) else {
            continue;
        };
        if exts
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(ext))
        {
            out.push(path);
        }
    }
}
