use crate::{BuildCommand, BuildTarget};
use duct::cmd;
use snafu::{FromString, ResultExt, Whatever};

#[cfg(feature = "p2e")]
use bebop_p2e::{BitstreamBuilder, BuildOutcome};

pub fn build(command: BuildCommand) -> Result<(), Whatever> {
    match command.target {
        BuildTarget::Verilator {
            rtl_dir,
            out_dir,
            diff,
            fast,
        } => {
            if !rtl_dir.is_dir() {
                let message = format!("RTL directory does not exist: {}", rtl_dir.display());
                return Err(Whatever::without_source(message));
            }
            if fast {
                return Err(Whatever::without_source(
                    "Verilator fast build is not supported yet".to_string(),
                ));
            }
            let rtl_dir = rtl_dir
                .canonicalize()
                .whatever_context("failed to canonicalize RTL directory")?;
            std::fs::create_dir_all(&out_dir).whatever_context("failed to create output directory")?;

            let features = if diff { "verilator,bemu" } else { "verilator" };
            println!("Building {features}: {} -> {}", rtl_dir.display(), out_dir.display());
            cmd!("cargo", "build", "--release", "--bin", "bebop", "--features", features)
                .env("VSRC_PATH", &rtl_dir)
                .run()
                .whatever_context("failed to build bebop")?;

            // copy the built executable to the output directory
            let dest = out_dir.join("bebop-verilator");
            std::fs::copy("target/release/bebop", &dest).whatever_context("failed to copy built executable")?;
            println!("Built executable: {}", dest.display());
            Ok(())
        }
        BuildTarget::P2e {
            rtl_dir,
            out_dir,
            diff,
            resume_post_route,
            stop_after,
        } => {
            if !rtl_dir.is_dir() {
                let message = format!("RTL directory does not exist: {}", rtl_dir.display());
                return Err(Whatever::without_source(message));
            }
            let rtl_dir = rtl_dir
                .canonicalize()
                .whatever_context("failed to canonicalize RTL directory")?;
            std::fs::create_dir_all(&out_dir).whatever_context("failed to create output directory")?;
            println!("Building p2e: {} -> {}", rtl_dir.display(), out_dir.display());
            let features = if diff { "p2e,bemu" } else { "p2e" };
            let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
            cmd!(
                "cargo",
                "build",
                "--release",
                "--manifest-path",
                &manifest,
                "--bin",
                "bebop",
                "--features",
                features
            )
            .env("VSRC_PATH", &rtl_dir)
            .env("OUT_PATH", &out_dir)
            .run()
            .whatever_context("failed to build p2e")?;

            #[cfg(feature = "p2e")]
            {
                let builder = BitstreamBuilder::new(out_dir.clone());
                let outcome = if resume_post_route {
                    builder.resume_post_route().map(|()| BuildOutcome::Runtime)
                } else {
                    builder.build(stop_after.as_deref())
                }
                .map_err(Whatever::without_source)?;
                if let BuildOutcome::Assessment(stage) = outcome {
                    println!(
                        "P2E resource assessment completed after {stage}; no bitstream or runnable case produced: {}",
                        out_dir.display()
                    );
                    return Ok(());
                }

                // copy the built executable to the output directory
                let dest = out_dir.join("bebop-p2e");
                let target_dir = std::env::var_os("CARGO_TARGET_DIR")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from("target"));
                let staged = out_dir.join(".bebop-p2e.new");
                let executable = target_dir.join("release/bebop");
                std::fs::copy(executable, &staged).whatever_context("failed to stage built executable")?;
                std::fs::rename(staged, &dest).whatever_context("failed to install built executable")?;
                println!("Built P2E runtime: {}", dest.display());
                Ok(())
            }
            #[cfg(not(feature = "p2e"))]
            {
                Err(Whatever::without_source(
                    "p2e builder is not compiled into this executable".to_string(),
                ))
            }
        }
    }
}
