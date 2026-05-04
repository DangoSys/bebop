use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const P2E_TOP: &str = "P2ETop";
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

    // Check if VVAC output already exists and can be reused
    let libctb_dst = out_dir.join("libvCtb.so");
    let vvac_dir = out_dir.join("vvacDir");
    let wrapper_lib = out_dir.join("libctb_wrapper.a");

    if libctb_dst.exists() && vvac_dir.exists() && wrapper_lib.exists() {
        println!("cargo:warning=Reusing existing VVAC build (libvCtb.so, vvacDir, and wrapper found)");
        println!("cargo:warning=To rebuild, set ARCH_CONFIG or delete {}", out_dir.display());
        link_vvac(&libctb_dst);
        println!("cargo:warning=✓ P2E build complete (reused existing VVAC)!");
        return;
    }

    // If no existing VVAC output, ARCH_CONFIG is required
    let config = match env::var("ARCH_CONFIG") {
        Ok(c) => c,
        Err(_) => {
            eprintln!("\n==========================================================");
            eprintln!("ERROR: ARCH_CONFIG environment variable is required");
            eprintln!("==========================================================");
            eprintln!();
            eprintln!("To build bebop-p2e, you must specify an architecture config:");
            eprintln!();
            eprintln!("  cargo build --features p2e --config=\"env.ARCH_CONFIG='sims.p2e.P2EToyConfig'\"");
            eprintln!();
            eprintln!("Or set it as an environment variable:");
            eprintln!();
            eprintln!("  export ARCH_CONFIG=sims.p2e.P2EToyConfig");
            eprintln!("  cargo build --features p2e");
            eprintln!();
            eprintln!("After the first build, subsequent builds will reuse the VVAC output.");
            eprintln!("==========================================================\n");
            panic!("ARCH_CONFIG not set");
        }
    };

    // Clean old build artifacts before starting fresh build
    println!("cargo:warning=Cleaning old build artifacts...");
    if out_dir.exists() {
        // Remove vvacDir
        let vvac_dir = out_dir.join("vvacDir");
        if vvac_dir.exists() {
            fs::remove_dir_all(&vvac_dir).ok();
            println!("cargo:warning=Removed old vvacDir");
        }

        // Remove .vm files
        if let Ok(entries) = fs::read_dir(&out_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("vm") {
                    fs::remove_file(&path).ok();
                    println!("cargo:warning=Removed old {}", path.file_name().unwrap().to_str().unwrap());
                }
            }
        }

        // Remove .log files
        if let Ok(entries) = fs::read_dir(&out_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("log") {
                    fs::remove_file(&path).ok();
                }
            }
        }
    }

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

    let mut vsrcs = collect_files(&build_dir, &["v", "sv"]);
    assert!(
        !vsrcs.is_empty(),
        "no Verilog/SystemVerilog files found under {}",
        build_dir.display()
    );

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

    let sourceme = manifest_dir.join(SOURCE_ME);
    assert_exists(&sourceme, "missing p2e sourceme script");

    println!("cargo:warning=Running vvac to generate DPI-C code...");
    run_vvac(&out_dir, &sourceme, &flist, P2E_TOP);

    println!("cargo:warning=Adding missing empty modules to VVAC filelist...");
    add_missing_empty_modules(&out_dir);

    println!("cargo:warning=Removing empty module instantiations from Verilog...");
    remove_empty_module_instantiations(&build_dir);

    // vvac already compiled and installed libvCtb.so, just copy it
    println!("cargo:warning=Copying libvCtb.so from vvac output...");
    let libctb_src = out_dir.join("vvacDir/runtimeDir/lib/lib_arm/libvCtb.so");
    let libctb_dst = out_dir.join("libvCtb.so");
    assert!(
        libctb_src.exists(),
        "libvCtb.so not found at {}. vvac may have failed.",
        libctb_src.display()
    );
    fs::copy(&libctb_src, &libctb_dst).expect("copy libvCtb.so");
    println!("cargo:warning=Copied libvCtb.so from {} to {}", libctb_src.display(), libctb_dst.display());

    println!("cargo:warning=Building C++ wrapper for Rust FFI...");
    build_cpp_wrapper(&manifest_dir, &out_dir);

    link_vvac(&libctb_dst);

    println!("cargo:warning=✓ P2E VVAC build complete!");
}

fn run_vvac(out_dir: &Path, sourceme: &Path, flist: &Path, top: &str) {
    // Run vvac directly with HPE toolchain environment (no Nix wrapper needed)
    // sourceme.sh now sources HPE GCC 8.3.0 which provides clang-format and other tools
    let script = format!(
        "cd {} && source {} && vvac -bc -f {} -top {} 2>&1 | tee {}",
        sh_quote(&out_dir.display().to_string()),
        sh_quote(&sourceme.display().to_string()),
        sh_quote(&flist.display().to_string()),
        sh_quote(top),
        sh_quote(&out_dir.join("vvac_build.log").display().to_string()),
    );

    let status = Command::new("bash")
        .arg("-lc")
        .arg(&script)
        .status()
        .expect("failed to execute vvac");

    if !status.success() {
        panic!(
            "vvac failed. Check log: {}",
            out_dir.join("vvac_build.log").display()
        );
    }
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
    let dpi_impl_src = manifest_dir.join("src/dpi_impl.cpp");
    let wrapper_obj = out_dir.join("ctb_wrapper.o");
    let dpi_impl_obj = out_dir.join("dpi_impl.o");
    let wrapper_lib = out_dir.join("libctb_wrapper.a");
    let dpi_so = out_dir.join("libp2e_dpi.so");

    println!("cargo:rerun-if-changed={}", wrapper_src.display());
    println!("cargo:rerun-if-changed={}", dpi_impl_src.display());

    // Compile C++ wrapper to object file
    // Use vvacDir/runtimeDir/include for headers (where vvac installs them)
    let include_dir = out_dir.join("vvacDir/runtimeDir/include");

    println!("cargo:warning=Compiling C++ wrapper with include dir: {}", include_dir.display());
    println!("cargo:warning=Wrapper source: {}", wrapper_src.display());
    println!("cargo:warning=DPI impl source: {}", dpi_impl_src.display());
    println!("cargo:warning=Output objects: {}, {}", wrapper_obj.display(), dpi_impl_obj.display());

    let include_arg = format!("-I{}", include_dir.display());

    // Compile ctb_wrapper.cpp
    let output = Command::new("g++")
        .args(&[
            "-c",
            "-fPIC",
            "-std=c++11",
            &include_arg,
            &wrapper_src.display().to_string(),
            "-o", &wrapper_obj.display().to_string(),
        ])
        .output()
        .expect("failed to compile C++ wrapper");

    if !output.status.success() {
        eprintln!("g++ stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("g++ stderr: {}", String::from_utf8_lossy(&output.stderr));
        panic!("C++ wrapper compilation failed");
    }

    // Compile dpi_impl.cpp
    let output = Command::new("g++")
        .args(&[
            "-c",
            "-fPIC",
            "-std=c++11",
            &dpi_impl_src.display().to_string(),
            "-o", &dpi_impl_obj.display().to_string(),
        ])
        .output()
        .expect("failed to compile DPI implementation");

    if !output.status.success() {
        eprintln!("g++ stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("g++ stderr: {}", String::from_utf8_lossy(&output.stderr));
        panic!("DPI implementation compilation failed");
    }

    // Create static library from both object files
    let status = Command::new("ar")
        .args(&[
            "rcs",
            &wrapper_lib.display().to_string(),
            &wrapper_obj.display().to_string(),
            &dpi_impl_obj.display().to_string(),
        ])
        .status()
        .expect("failed to create wrapper library");

    assert!(status.success(), "wrapper library creation failed");

    // Create shared library for DPI-C functions (for VVAC runtime)
    println!("cargo:warning=Creating DPI-C shared library: {}", dpi_so.display());
    let output = Command::new("g++")
        .args(&[
            "-shared",
            "-fPIC",
            "-std=c++11",
            &dpi_impl_obj.display().to_string(),
            "-o", &dpi_so.display().to_string(),
        ])
        .output()
        .expect("failed to create DPI shared library");

    if !output.status.success() {
        eprintln!("g++ stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("g++ stderr: {}", String::from_utf8_lossy(&output.stderr));
        panic!("DPI shared library creation failed");
    }

    // Copy DPI shared library to VVAC runtime directory
    let vvac_lib_dir = out_dir.join("vvacDir/runtimeDir/lib/lib_arm");
    if vvac_lib_dir.exists() {
        let target_so = vvac_lib_dir.join("libp2e_dpi.so");
        if let Err(e) = fs::copy(&dpi_so, &target_so) {
            println!("cargo:warning=Failed to copy DPI library to VVAC: {}", e);
        } else {
            println!("cargo:warning=Copied DPI library to: {}", target_so.display());
        }
    }

    // Link the wrapper library
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=ctb_wrapper");
    println!("cargo:rustc-link-lib=dylib=stdc++");
}

fn link_vvac(libctb: &Path) {
    let lib_dir = libctb.parent().expect("libvCtb.so parent directory");
    let lib_dir_str = lib_dir.display().to_string();

    println!("cargo:warning=Setting RPATH to: {}", lib_dir_str);
    println!("cargo:warning=libvCtb.so location: {}", libctb.display());

    // Also copy libvCtb.so to target directory for easier access
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
    println!("cargo:rustc-link-lib=dylib=stdc++");

    // Use $ORIGIN to find library relative to executable
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir_str);
    println!("cargo:rustc-link-arg=-Wl,--enable-new-dtags");
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

fn add_missing_empty_modules(out_dir: &Path) {
    // Add missing empty modules to VVAC filelist
    // These modules are generated by VVAC but not included in the filelist
    let filelist_path = out_dir.join("vvacDir/vvac_by_mod/filelist");
    if !filelist_path.exists() {
        println!("cargo:warning=VVAC filelist not found, skipping");
        return;
    }

    let content = fs::read_to_string(&filelist_path)
        .expect("Failed to read VVAC filelist");

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

        fs::write(&filelist_path, new_content)
            .expect("Failed to write updated VVAC filelist");
        println!("cargo:warning=Added {} missing empty modules to VVAC filelist", added_count);
    } else {
        println!("cargo:warning=No missing empty modules found");
    }
}

fn remove_empty_module_instantiations(build_dir: &Path) {
    // Remove empty module instantiations from DigitalTop.sv
    // These modules have no ports and no connections, so they can be safely removed
    let digital_top = build_dir.join("DigitalTop.sv");
    if !digital_top.exists() {
        println!("cargo:warning=DigitalTop.sv not found, skipping empty module removal");
        return;
    }

    let content = fs::read_to_string(&digital_top)
        .expect("Failed to read DigitalTop.sv");

    // Pattern to match empty module instantiations:
    // IntSyncCrossingSource_n1x1_Registered intsource ();
    // NullIntSource null_int_source ();
    let patterns = [
        r"IntSyncCrossingSource_n1x1_Registered\s+\w+\s*\(\s*\)\s*;",
        r"NullIntSource\s+\w+\s*\(\s*\)\s*;",
    ];

    let mut new_content = content.clone();
    let mut removed_count = 0;

    for pattern in &patterns {
        let re = regex::Regex::new(pattern).expect("Invalid regex pattern");
        let matches: Vec<_> = re.find_iter(&new_content).collect();

        if !matches.is_empty() {
            for m in &matches {
                println!("cargo:warning=Removing empty module instantiation: {}", m.as_str().trim());
                removed_count += 1;
            }
            new_content = re.replace_all(&new_content, "").to_string();
        }
    }

    if removed_count > 0 {
        fs::write(&digital_top, new_content)
            .expect("Failed to write updated DigitalTop.sv");
        println!("cargo:warning=Removed {} empty module instantiations from DigitalTop.sv", removed_count);
    } else {
        println!("cargo:warning=No empty module instantiations found in DigitalTop.sv");
    }
}
