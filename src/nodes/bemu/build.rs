use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SPIKE_REPO: &str = "https://github.com/riscv-software-src/riscv-isa-sim.git";
const SPIKE_TAG: &str = "master";

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));

    let spike_dir = manifest_dir.join("spike");
    let spike_install_dir = out_dir.join("spike_install");
    let spike_build_dir = out_dir.join("spike_build");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/buckyball_rocc.cc");
    println!("cargo:rerun-if-changed=src/spike_wrapper.cc");

    // Download spike if not exists
    if !spike_dir.exists() || !spike_dir.join("configure.ac").exists() {
        println!("cargo:warning=Downloading Spike from {}", SPIKE_REPO);
        download_spike(&spike_dir);
    }

    // Copy buckyball_rocc.cc to spike customext
    let customext_dir = spike_dir.join("customext");
    fs::copy(
        manifest_dir.join("src/buckyball_rocc.cc"),
        customext_dir.join("buckyball_rocc.cc")
    ).expect("copy buckyball_rocc.cc");

    // Patch customext.mk.in to include buckyball_rocc.cc
    let customext_mk = customext_dir.join("customext.mk.in");
    let mk_content = fs::read_to_string(&customext_mk).expect("read customext.mk.in");
    if !mk_content.contains("buckyball_rocc.cc") {
        let patched = mk_content.replace(
            "customext_srcs = \\\n\tdummy_rocc.cc \\\n\tcflush.cc \\\n",
            "customext_srcs = \\\n\tdummy_rocc.cc \\\n\tcflush.cc \\\n\tbuckyball_rocc.cc \\\n"
        );
        fs::write(&customext_mk, patched).expect("write customext.mk.in");
    }

    // Build and install spike
    build_spike(&spike_dir, &spike_build_dir, &spike_install_dir);

    // Compile spike_wrapper.cc (after spike is installed)
    cc::Build::new()
        .cpp(true)
        .file("src/spike_wrapper.cc")
        .include(spike_install_dir.join("include/riscv"))
        .include(spike_install_dir.join("include/fesvr"))
        .flag("-std=c++17")
        .compile("spike_wrapper");

    // Link spike libraries
    println!("cargo:rustc-link-search=native={}/lib", spike_install_dir.display());
    println!("cargo:rustc-link-lib=dylib=riscv");
    println!("cargo:rustc-link-lib=dylib=disasm");
    println!("cargo:rustc-link-lib=dylib=softfloat");
    println!("cargo:rustc-link-lib=dylib=fesvr");
    println!("cargo:rustc-link-lib=dylib=customext");
    println!("cargo:rustc-link-lib=dylib=stdc++");
}

fn download_spike(spike_dir: &Path) {
    // Remove existing directory if it's incomplete
    if spike_dir.exists() {
        fs::remove_dir_all(spike_dir).expect("remove incomplete spike dir");
    }

    // Clone spike repository
    let clone_status = Command::new("git")
        .arg("clone")
        .arg("--depth=1")
        .arg("--branch")
        .arg(SPIKE_TAG)
        .arg(SPIKE_REPO)
        .arg(spike_dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("run git clone");

    if !clone_status.success() {
        panic!("git clone spike failed with status {}", clone_status);
    }

    // Run autoreconf to generate configure script
    let autoreconf_status = Command::new("autoreconf")
        .current_dir(spike_dir)
        .arg("-i")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("run autoreconf");

    if !autoreconf_status.success() {
        panic!("autoreconf failed with status {}", autoreconf_status);
    }

    println!("cargo:warning=Spike downloaded and configured successfully");
}

fn build_spike(spike_dir: &Path, build_dir: &Path, install_dir: &Path) {
    fs::create_dir_all(build_dir).expect("create spike build dir");
    fs::create_dir_all(install_dir).expect("create spike install dir");

    println!("cargo:warning=Building spike from {}", spike_dir.display());

    let configure_status = Command::new(spike_dir.join("configure"))
        .current_dir(build_dir)
        .arg(format!("--prefix={}", install_dir.display()))
        .arg("--with-boost=no")
        .arg("--with-boost-asio=no")
        .arg("--with-boost-regex=no")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("run spike configure");

    if !configure_status.success() {
        panic!("spike configure failed with status {}", configure_status);
    }

    let make_status = Command::new("make")
        .current_dir(build_dir)
        .arg("-j")
        .arg(env::var("NUM_JOBS").unwrap_or_else(|_| "4".to_string()))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("run spike make");

    if !make_status.success() {
        panic!("spike make failed with status {}", make_status);
    }

    let install_status = Command::new("make")
        .current_dir(build_dir)
        .arg("install")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("run spike make install");

    if !install_status.success() {
        panic!("spike make install failed with status {}", install_status);
    }

    println!("cargo:warning=Spike installed to {}", install_dir.display());
}
