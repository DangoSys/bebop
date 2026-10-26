use crate::{BuildCommand, BuildTarget};
use duct::cmd;
use snafu::{FromString, ResultExt, Whatever};
use std::path::{Path, PathBuf};

#[cfg(feature = "p2e")]
use bebop_p2e::BitstreamBuilder;

fn build_runner(
    design_dir: &Path,
    entry_dir: &Path,
    backend: &str,
    features: &str,
    rtl_dir: Option<&Path>,
    case_dir: Option<&Path>,
) -> Result<PathBuf, Whatever> {
    let design_dir = design_dir
        .canonicalize()
        .whatever_context("failed to locate design directory")?;
    let output = design_dir.join("output");
    let manifest = entry_dir.join("Cargo.toml");
    let target = output.join("bebop").join(backend).join("target");
    let name = format!("bebop-{backend}");
    let mut command = cmd!(
        "cargo",
        "build",
        "--release",
        "--manifest-path",
        &manifest,
        "--target-dir",
        &target,
        "--bin",
        &name,
        "--features",
        features
    );
    if let Some(rtl_dir) = rtl_dir {
        command = command.env("VSRC_PATH", rtl_dir);
    }
    if let Some(case_dir) = case_dir {
        command = command.env("OUT_PATH", case_dir);
    }
    command.run().whatever_context("failed to build design runner")?;
    let bin_dir = output.join("bin");
    std::fs::create_dir_all(&bin_dir).whatever_context("failed to create design binary directory")?;
    let executable = bin_dir.join(&name);
    std::fs::copy(target.join("release").join(&name), &executable)
        .whatever_context("failed to install design runner")?;
    if backend == "p2e" {
        let library = target.join("release/deps/libbebop_p2e.so");
        let libraries = output.join("lib");
        std::fs::create_dir_all(&libraries).whatever_context("failed to create design library directory")?;
        std::fs::copy(&library, libraries.join("libbebop_p2e.so"))
            .whatever_context("failed to install design Rust DPI library")?;
        let case = case_dir.expect("P2E build requires a case directory");
        std::fs::copy(&library, case.join("vvacDir/runtimeDir/lib/lib_arm/libbebop_p2e.so"))
            .whatever_context("failed to install case Rust DPI library")?;
    }
    println!("Built design runner: {}", executable.display());
    Ok(executable)
}

pub fn build(command: BuildCommand) -> Result<(), Whatever> {
    match command.target {
        BuildTarget::Vcs {
            rtl_dir,
            artifact_dir,
            jobs,
        } => {
            bebop_vcs::Builder {
                rtl_dir,
                artifact_dir,
                jobs,
            }
            .build()?;
        }
        BuildTarget::Bemu { design_dir, entry_dir } => {
            build_runner(&design_dir, &entry_dir, "bemu", "bemu", None, None)?;
        }
        BuildTarget::Verilator {
            design_dir,
            entry_dir,
            rtl_dir,
            diff,
            fast,
        } => {
            if fast {
                return Err(Whatever::without_source(
                    "Verilator fast build is not supported yet".to_string(),
                ));
            }
            let rtl_dir = rtl_dir
                .canonicalize()
                .whatever_context("failed to locate RTL directory")?;
            let features = if diff { "verilator,bemu" } else { "verilator" };
            build_runner(&design_dir, &entry_dir, "verilator", features, Some(&rtl_dir), None)?;
        }
        BuildTarget::P2e {
            design_dir,
            entry_dir,
            rtl_dir,
            out_dir,
            diff,
            itrace,
            mtrace,
        } => {
            let rtl_dir = rtl_dir
                .canonicalize()
                .whatever_context("failed to locate RTL directory")?;
            std::fs::create_dir_all(&out_dir).whatever_context("failed to create P2E case directory")?;
            let out_dir = out_dir
                .canonicalize()
                .whatever_context("failed to locate P2E case directory")?;
            let mut features = vec!["p2e"];
            if diff {
                features.push("bemu");
            }
            if itrace {
                features.push("bebop-p2e/itrace");
            }
            if mtrace {
                features.push("bebop-p2e/mtrace");
            }
            let executable = build_runner(
                &design_dir,
                &entry_dir,
                "p2e",
                &features.join(","),
                Some(&rtl_dir),
                Some(&out_dir),
            )?;
            #[cfg(feature = "p2e")]
            {
                BitstreamBuilder::new(out_dir.clone())
                    .build()
                    .map_err(Whatever::without_source)?;
                std::fs::copy(executable, out_dir.join("bebop-p2e"))
                    .whatever_context("failed to install P2E case runner")?;
            }
            #[cfg(not(feature = "p2e"))]
            {
                let _ = executable;
                return Err(Whatever::without_source(
                    "P2E build requires the design package's p2e feature".to_string(),
                ));
            }
        }
    }
    Ok(())
}
