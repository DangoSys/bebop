//! Build support for simulation entries: what a design.pb asks the entry to link,
//! and how the runners the entry builds are linked.
use prost::Message;
use std::{
    ffi::OsStr,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

mod config {
    include!(concat!(env!("OUT_DIR"), "/buckyball.config.rs"));
}

/// The design.pb of the simulation being built. A simulation is built in
/// `output/sims/<selection>/emu/<backend>/target`; its design.pb is
/// `output/sims/<selection>/generated/design.pb`.
fn design_pb(out_dir: &Path) -> PathBuf {
    let target = out_dir
        .ancestors()
        .find(|path| path.file_name() == Some(OsStr::new("target")))
        .expect("a simulation is built in the target directory of its output");
    target
        .join("../../../generated/design.pb")
        .canonicalize()
        .expect("the simulation has no generated design.pb")
}

/// For the build script of a simulation entry: copies the design.pb of the
/// simulation being built to `$OUT_DIR/design.pb`, and writes `$OUT_DIR/balls.rs`,
/// one module per Ball of the design, compiled from the model its `emu_lib`
/// names. The models register themselves in `bebop_bemu::BALLS`.
pub fn balls() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let design_pb = design_pb(&out);
    let bytes = fs::read(&design_pb).unwrap();
    let design = config::Design::decode(bytes.as_slice()).unwrap();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../..")
        .canonicalize()
        .unwrap();
    let mut modules = String::new();
    // A design without Balls has no bemu message.
    for ball in design.bemu.unwrap_or_default().balls {
        let source = repo.join(&ball.emu_lib);
        writeln!(modules, "#[path = {:?}]\nmod {};", source, ball.component).unwrap();
        println!("cargo:rerun-if-changed={}", source.parent().unwrap().display());
    }
    fs::write(out.join("balls.rs"), modules).unwrap();
    fs::write(out.join("design.pb"), bytes).unwrap();
    println!("cargo:rerun-if-changed={}", design_pb.display());
}

/// Link settings for the runners a simulation entry builds: the Verilator
/// runner finds the nix environment's shared libraries, the P2E runner the
/// libraries of its case directory.
pub fn runner_links() {
    if std::env::var("CARGO_FEATURE_VERILATOR").is_ok() {
        let riscv = std::env::var("RISCV").expect("RISCV must be set by the nix development environment");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{riscv}/lib");
        println!("cargo:rustc-link-arg=-Wl,--enable-new-dtags");
    }
    if std::env::var("CARGO_FEATURE_P2E").is_ok() {
        println!("cargo:rerun-if-env-changed=OUT_PATH");
        let out_dir = std::env::var("OUT_PATH").expect("OUT_PATH must name the P2E case directory");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{out_dir}/vvacDir/runtimeDir/lib/lib_arm");
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{out_dir}");
        println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
    }
}
