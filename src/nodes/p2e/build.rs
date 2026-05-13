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
//===-----------------------------------------------------------------===//-----===//
//
//===-----------------------------------------------------------------===//-----===//

use duct::cmd;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const P2E_TOP: &str = "P2ETop";
const SOURCE_ME: &str = "sourceme.sh";

fn main() {
    println!("cargo:rustc-check-cfg=cfg(vvac_linked)");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=VSRC_PATH");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = manifest_dir
        .ancestors()
        .nth(3)
        .expect("p2e crate should live under bebop/src/nodes/p2e")
        .join("out");
    let libctb_dst = out_dir.join("libvCtb.so");

    // Check if libvCtb.so already exists (from previous build)
    if libctb_dst.exists() {
        println!("cargo:warning=Found existing libvCtb.so, skipping VVAC build");

        // Always rebuild C++ wrapper to ensure it's up to date
        println!("cargo:warning=Building C++ wrapper for Rust FFI...");
        build_cpp_wrapper(&manifest_dir, &out_dir);

        link_vvac(&libctb_dst);
        return;
    }

    // VSRC_PATH is required for building
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
    assert_exists(&sourceme, "missing p2e sourceme script");

    //===-----------------------------------------------------------------===//-----===//
    // 1. Clean old build artifacts every time before build
    // if you want to save your old build artifacts (like bitstream.bit),
    // it suggest setting --out-dir to a different directory (default is ./out)
    //===-----------------------------------------------------------------===//-----===//
    println!("cargo:warning=Cleaning old build artifacts...");
    clean_build_artifacts(&out_dir);

    //===-----------------------------------------------------------------===//-----===//
    // 2. Pre-process Verilog sources
    //===-----------------------------------------------------------------===//-----===//
    assert_exists(
        &build_dir,
        &format!("missing Verilog source directory at VSRC_PATH={}", vsrc_path),
    );
    assert_exists(
        &build_dir.join(format!("{P2E_TOP}.sv")),
        &format!("VSRC_PATH={} does not look like a P2E build", vsrc_path),
    );
    let mut vsrcs = collect_files(&build_dir, &["v", "sv"]);

    // Add XEPIC DDR4 IP stub
    // Use the stub from src/ddr/ip directory
    let ddr4_stub = manifest_dir.join("src/ddr/ip/xepic_ddr4_dc1_stub.sv");
    assert_exists(&ddr4_stub, "xepic_ddr4_dc1 stub not found");
    vsrcs.push(ddr4_stub);
    println!("cargo:warning=Added xepic_ddr4_dc1 stub from src/ddr/ip");
    println!("cargo:rerun-if-changed={}", build_dir.display());

    fs::create_dir_all(&out_dir).expect("create p2e out directory");
    let flist = out_dir.join("p2e_vvac_filelist.f");
    write_flist(&flist, &vsrcs);

    println!("cargo:warning=Removing empty module instantiations from Verilog...");
    remove_empty_module_instantiations(&build_dir);

    //===-----------------------------------------------------------------===//-----===//
    // 3. Run vvac to generate vvacDir and libvCtb.so
    //
    // why need rebuild?
    //  vvac will generate empty module stubs, we need to add them to the filelist
    //
    //===-----------------------------------------------------------------===//-----===//
    println!("cargo:warning=Running vvac (first pass) to generate empty module stubs...");
    run_vvac(&out_dir, &sourceme, &flist, P2E_TOP);

    println!("cargo:warning=Adding missing empty modules to VVAC filelist...");
    let needs_rebuild = add_missing_empty_modules(&out_dir);

    if needs_rebuild {
        println!("cargo:warning=Running vvac (second pass) with complete filelist...");
        run_vvac(&out_dir, &sourceme, &flist, P2E_TOP);
    }

    println!("cargo:warning=Copying libvCtb.so from vvac output...");
    let libctb_src = out_dir.join("vvacDir/runtimeDir/lib/lib_arm/libvCtb.so");
    let libctb_dst = out_dir.join("libvCtb.so");
    assert_exists(&libctb_src, "libvCtb.so not found. vvac may have failed");
    fs::copy(&libctb_src, &libctb_dst).expect("copy libvCtb.so");
    println!(
        "cargo:warning=Copied libvCtb.so from {} to {}",
        libctb_src.display(),
        libctb_dst.display()
    );

    //===-----------------------------------------------------------------===//-----===//
    // 3.5. Fix VVAC library RPATH for ABI compatibility
    //===-----------------------------------------------------------------===//-----===//
    println!("cargo:warning=Fixing VVAC library RPATH for C++ ABI compatibility...");
    fix_vvac_library_rpath(&out_dir);

    //===-----------------------------------------------------------------===//-----===//
    // 4. Build Cpp from vvac to export DPI-C functions (CTB managemen) to Rust
    //===-----------------------------------------------------------------===//-----===//
    println!("cargo:warning=Building C++ wrapper for Rust FFI...");
    build_cpp_wrapper(&manifest_dir, &out_dir);

    //===-----------------------------------------------------------------===//-----===//
    // 5. Link Cpp wrapper with bebop
    //===-----------------------------------------------------------------===//-----===//
    println!("cargo:warning=Linking vvac and C++ wrapper...");
    link_vvac(&libctb_dst);

    println!("cargo:warning=P2E VVAC build complete!");
}

fn run_vvac(out_dir: &Path, sourceme: &Path, flist: &Path, top: &str) {
    let vvac_cmd = format!(
        "source {} && vvac -bc -f {} -top {}",
        sourceme.display(),
        flist.display(),
        top
    );

    cmd!("bash", "-c", &vvac_cmd)
        .dir(out_dir)
        .stdout_to_stderr()
        .run()
        .unwrap_or_else(|e| {
            panic!(
                "vvac failed: {}. Check log: {}",
                e,
                out_dir.join("vvac_build.log").display()
            )
        });
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
    let wrapper_src = manifest_dir.join("src/ctb/ctb_wrapper.cpp");
    let wrapper_obj = out_dir.join("ctb_wrapper.o");
    let wrapper_lib = out_dir.join("libctb_wrapper.a");

    println!("cargo:rerun-if-changed={}", wrapper_src.display());

    let include_dir = out_dir.join("vvacDir/runtimeDir/include");
    let include_arg = format!("-I{}", include_dir.display());

    // CRITICAL: Use VVAC's libstdc++ for C++ wrapper to ensure ABI compatibility
    let vvac_lib_dir = out_dir.join("vvacDir/runtimeDir/lib/lib_arm");
    let vvac_libstdcxx = vvac_lib_dir.join("libstdc++.so.6");

    println!("cargo:warning=Compiling C++ wrapper include: {}", include_dir.display());
    println!("cargo:warning=Wrapper source: {}", wrapper_src.display());
    println!("cargo:warning=Output object: {}", wrapper_obj.display());
    println!("cargo:warning=Using VVAC's libstdc++: {}", vvac_libstdcxx.display());

    // Compile ctb_wrapper.cpp with VVAC's libstdc++
    // CRITICAL: Use -Wl,-rpath to ensure the wrapper uses VVAC's libstdc++ at runtime
    cmd!(
        "g++",
        "-c",
        "-fPIC",
        "-std=c++11",
        &include_arg,
        wrapper_src.to_str().unwrap(),
        "-o",
        wrapper_obj.to_str().unwrap()
    )
    .run()
    .expect("failed to compile C++ wrapper");

    // Create static library from wrapper object file
    cmd!(
        "ar",
        "rcs",
        wrapper_lib.to_str().unwrap(),
        wrapper_obj.to_str().unwrap()
    )
    .run()
    .expect("failed to create wrapper library");

    // Link the wrapper library
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=ctb_wrapper");

    // CRITICAL: Add VVAC's lib directory to link search path so wrapper can find libstdc++
    println!("cargo:rustc-link-search=native={}", vvac_lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=stdc++");
}

fn link_vvac(libctb: &Path) {
    let lib_dir = libctb.parent().expect("libvCtb.so parent directory");
    let lib_dir_str = lib_dir.display().to_string();

    println!("cargo:warning=Setting RPATH to: {}", lib_dir_str);
    println!("cargo:warning=libvCtb.so location: {}", libctb.display());

    // Copy libvCtb.so to target directory for easier access
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let target_lib = out_dir.join("../../../libvCtb.so");
    if let Err(e) = fs::copy(libctb, &target_lib) {
        println!("cargo:warning=Failed to copy libvCtb.so to target: {}", e);
    } else {
        println!("cargo:warning=Copied libvCtb.so to {}", target_lib.display());
    }

    println!("cargo:rustc-link-search=native={}", lib_dir_str);
    println!("cargo:rustc-link-lib=dylib=vCtb");
    println!("cargo:rustc-link-lib=static=ctb_wrapper");

    // NOTE: Do NOT link libstdc++ here - let Rust handle it
    // VVAC libraries will find their own libstdc++.so.6.0.25 via LD_LIBRARY_PATH

    // Use $ORIGIN to find library relative to executable
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir_str);
    println!("cargo:rustc-link-arg=-Wl,--enable-new-dtags");

    // CRITICAL: Enable lazy binding so undefined symbols in libvCtb.so
    // (scu_uart_write, scu_sim_exit) are resolved at runtime, not at load time.
    // This allows LD_PRELOAD to work correctly.
    println!("cargo:rustc-link-arg=-Wl,-z,lazy");

    // CRITICAL: Export all symbols from the main executable so libvCtb.so
    // can find DPI-C functions (scu_uart_write, scu_sim_exit) at runtime.
    // Without this, libvCtb.so can only see symbols from itself, not from
    // other shared libraries or the main executable.
    println!("cargo:rustc-link-arg=-Wl,--export-dynamic");

    println!("cargo:rustc-cfg=vvac_linked");
}

fn clean_build_artifacts(out_dir: &Path) {
    if !out_dir.exists() {
        return;
    }

    let vvac_dir = out_dir.join("vvacDir");
    if vvac_dir.exists() {
        fs::remove_dir_all(&vvac_dir).ok();
        println!("cargo:warning=Removed old vvacDir");
    }

    if let Ok(entries) = fs::read_dir(out_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(OsStr::to_str) {
                if ext == "vm" || ext == "log" {
                    fs::remove_file(&path).ok();
                }
            }
        }
    }
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
        if exts.iter().any(|candidate| candidate.eq_ignore_ascii_case(ext)) {
            out.push(path);
        }
    }
}

fn add_missing_empty_modules(out_dir: &Path) -> bool {
    // Add missing empty modules to VVAC filelist
    // These modules are generated by VVAC but not included in the filelist
    // Returns true if modules were added (indicating a rebuild is needed)
    let filelist_path = out_dir.join("vvacDir/vvac_by_mod/filelist");
    if !filelist_path.exists() {
        println!("cargo:warning=VVAC filelist not found, skipping");
        return false;
    }

    let content = fs::read_to_string(&filelist_path).expect("Failed to read VVAC filelist");

    let vvac_dir = out_dir.join("vvacDir/vvac_by_mod");

    // Known empty modules that need to be added
    let empty_modules = [
        "work_DebugCustomXbar.sv",
        "work_IntSyncCrossingSource_n1x1_Registered.sv",
        "work_NullIntSource.sv",
    ];

    let mut added_count = 0;
    let mut new_lines = Vec::new();

    for module in &empty_modules {
        let file_path = vvac_dir.join(module);

        // Check if file exists and is not already in filelist
        if file_path.exists() && !content.contains(module) {
            new_lines.push(format!("./{}", module));
            println!("cargo:warning=Adding missing empty module to filelist: {}", module);
            added_count += 1;
        }
    }

    if added_count > 0 {
        // Append the new modules to the filelist
        let mut new_content = content;
        if !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(&new_lines.join("\n"));
        new_content.push('\n');

        fs::write(&filelist_path, new_content).expect("Failed to write updated VVAC filelist");
        println!("cargo:warning=Added {} empty modules to VVAC filelist", added_count);
        true
    } else {
        println!("cargo:warning=No missing empty modules found");
        false
    }
}

fn remove_empty_module_instantiations(build_dir: &Path) {
    let digital_top = build_dir.join("DigitalTop.sv");
    if !digital_top.exists() {
        println!("cargo:warning=DigitalTop.sv not found, skipping empty module removal");
        return;
    }

    let content = fs::read_to_string(&digital_top).expect("Failed to read DigitalTop.sv");

    // Empty module patterns: ModuleName instance_name ();
    let empty_modules = [
        "IntSyncCrossingSource_n1x1_Registered",
        "NullIntSource",
        "IntXbar_i0_o0",
    ];

    let mut removed_count = 0;
    let new_content: String = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            let should_remove = empty_modules
                .iter()
                .any(|module| trimmed.contains(module) && trimmed.ends_with("();"));

            if should_remove {
                println!("cargo:warning=Removing empty module instantiation: {}", trimmed);
                removed_count += 1;
            }
            !should_remove
        })
        .collect::<Vec<_>>()
        .join("\n");

    if removed_count > 0 {
        fs::write(&digital_top, new_content).expect("Failed to write updated DigitalTop.sv");
        println!(
            "cargo:warning=Removed {} empty module instantiations from DigitalTop.sv",
            removed_count
        );
    } else {
        println!("cargo:warning=No empty module instantiations found in DigitalTop.sv");
    }
}

fn fix_vvac_library_rpath(out_dir: &Path) {
    // CRITICAL: Fix RPATH of VVAC libraries to ensure C++ ABI compatibility
    // VVAC libraries (libtbppeer.so, etc.) were compiled with libstdc++.so.6.0.25
    // but their RPATH points to non-existent build-time paths.
    // We need to set RPATH to $ORIGIN so they load libstdc++ from their own directory.

    let vvac_lib_dir = out_dir.join("vvacDir/runtimeDir/lib/lib_arm");
    if !vvac_lib_dir.exists() {
        println!("cargo:warning=VVAC lib directory not found, skipping RPATH fix");
        return;
    }

    // List of VVAC libraries that need RPATH fix
    let libraries = ["libtbppeer.so", "libvCtb.so", "libvmri.so"];

    for lib_name in &libraries {
        let lib_path = vvac_lib_dir.join(lib_name);
        if !lib_path.exists() {
            println!("cargo:warning={} not found, skipping", lib_name);
            continue;
        }

        println!("cargo:warning=Fixing RPATH for {}", lib_name);

        // Use patchelf to set RPATH to $ORIGIN
        let result = cmd!("patchelf", "--set-rpath", "$ORIGIN", lib_path.to_str().unwrap())
            .run();

        match result {
            Ok(_) => println!("cargo:warning=Successfully fixed RPATH for {}", lib_name),
            Err(e) => println!("cargo:warning=Failed to fix RPATH for {}: {}", lib_name, e),
        }
    }
}
