use super::{config, paths};
use prost::Message;
use std::{
    ffi::OsStr,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

/// Resolve the simulation entry registered in the selected design.pb.
pub fn runner_manifest(design_pb: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let design = config::Design::decode(fs::read(design_pb)?.as_slice())?;
    let simulation = design.simulation.ok_or("design.pb has no simulation entry")?;
    let repo = paths::buckyball(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.."));
    Ok(repo.join(simulation.emu).join("Cargo.toml"))
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
/// The parameter schema of every parameterized Ball instance of the design,
/// `<source_path>/configs/<component>_params.proto`, is compiled to
/// `$OUT_DIR/buckyball.balls.<component>.rs` for its model to include.
pub fn balls() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let design_pb = design_pb(&out);
    let bytes = fs::read(&design_pb).unwrap();
    let design = config::Design::decode(bytes.as_slice()).unwrap();
    let repo = paths::buckyball(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.."));
    let thread_balls = design.threads.iter().flat_map(|thread| &thread.balldomain);
    let core_balls = design
        .device
        .iter()
        .flat_map(|device| &device.cores)
        .flat_map(|core| &core.accelerator)
        .flat_map(|accelerator| &accelerator.balldomain);
    let mut schemas = Vec::new();
    for ball in thread_balls.chain(core_balls).flat_map(|domain| &domain.balls) {
        if ball.parameters.is_some() {
            let schema = repo
                .join(&ball.source_path)
                .join("configs")
                .join(format!("{}_params.proto", ball.component));
            if !schemas.contains(&schema) {
                println!("cargo:rerun-if-changed={}", schema.display());
                schemas.push(schema);
            }
        }
    }
    if !schemas.is_empty() {
        let includes = schemas.iter().map(|schema| schema.parent().unwrap()).collect::<Vec<_>>();
        prost_build::compile_protos(&schemas, &includes).unwrap();
    }
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
