use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const P2E_TOP: &str = "P2EHarness";
const VVAC_REL_LIB_DIR: &str = "out/vvacDir/runtimeDir/lib/lib_arm";
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
    run_vvac(&out_dir, &sourceme, &flist, P2E_TOP);

    let vvac_lib_dir = find_vvac_lib_dir(&bebop_root).unwrap_or_else(|| {
        panic!(
            "libvCtb.so not found after vvac under {}",
            out_dir.display()
        )
    });
    link_vvac(&vvac_lib_dir);
}

fn run_vvac(out_dir: &Path, sourceme: &Path, flist: &Path, top: &str) {
    // Create a wrapper bin directory with a clang-format wrapper that clears
    // LD_LIBRARY_PATH to prevent HPE's old libstdc++.so.6 from breaking Nix's clang-format
    let wrapper_bin = out_dir.join("wrapper_bin");
    fs::create_dir_all(&wrapper_bin).expect("create wrapper bin directory");

    let wrapper_path = wrapper_bin.join("clang-format");
    let wrapper_content = r#"#!/bin/bash
# Wrapper to isolate clang-format from HPE's old libstdc++.so.6
# Find the real clang-format (skip this wrapper)
real_clang_format=$(PATH="${PATH#*:}" command -v clang-format 2>/dev/null)
if [ -z "$real_clang_format" ]; then
    echo "Error: clang-format not found in PATH" >&2
    exit 127
fi
# Clear LD_LIBRARY_PATH to prevent HPE lib pollution
unset LD_LIBRARY_PATH
exec "$real_clang_format" "$@"
"#;
    fs::write(&wrapper_path, wrapper_content).expect("write clang-format wrapper");
    fs::set_permissions(&wrapper_path, fs::Permissions::from_mode(0o755))
        .expect("chmod clang-format wrapper");

    // Create log file for vvac output
    let log_file = out_dir.join("vvac_build.log");
    let log_fd = fs::File::create(&log_file).expect("create vvac log file");

    let script = format!(
        "export PATH='{}':\"$PATH\" && source '{}' && vvac -bc -f '{}' -top '{}' 2>&1 | tee '{}'",
        sh_quote(&wrapper_bin.display().to_string()),
        sh_quote(&sourceme.display().to_string()),
        sh_quote(&flist.display().to_string()),
        sh_quote(top),
        sh_quote(&log_file.display().to_string()),
    );

    println!("cargo:warning=VVAC log will be written to: {}", log_file.display());
    println!("cargo:warning=You can monitor it with: tail -f {}", log_file.display());

    let mut cmd = Command::new("bash");
    cmd.stdout(Stdio::from(log_fd.try_clone().expect("clone log fd")))
        .stderr(Stdio::from(log_fd))
        .current_dir(out_dir)
        .arg("-lc")
        .arg(script);

    let status = match cmd.status() {
        Ok(status) => status,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            panic!("bash not found; cannot source p2e environment before running vvac")
        }
        Err(error) => panic!("failed to execute vvac through p2e sourceme.sh: {error}"),
    };
    assert!(status.success(), "vvac failed with status {status}");
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

fn link_vvac(lib_dir: &Path) {
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=vCtb");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    println!("cargo:rustc-cfg=vvac_linked");
    println!(
        "cargo:warning=Linking with libvCtb.so from {}",
        lib_dir.display()
    );
}

fn find_vvac_lib_dir(bebop_root: &Path) -> Option<PathBuf> {
    candidate_vvac_lib_dirs(bebop_root)
        .into_iter()
        .find(|dir| dir.join("libvCtb.so").exists())
}

fn candidate_vvac_lib_dirs(bebop_root: &Path) -> Vec<PathBuf> {
    vec![
        // Workspace builds use bebop/out, matching Verilator's repo-root flow.
        bebop_root.join(VVAC_REL_LIB_DIR),
    ]
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
