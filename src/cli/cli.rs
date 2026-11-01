use clap::{Args, Parser, Subcommand};
use std::{error::Error, path::PathBuf, process::Command};

use crate::paths;

#[derive(Parser)]
#[command(name = "bebop-bemu")]
struct Cli {
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    Config(Selection),
    Build(Selection),
    Run {
        #[command(flatten)]
        selection: Selection,
        #[arg(long)]
        elf: PathBuf,
    },
}

#[derive(Args)]
struct Selection {
    #[arg(long)]
    design: String,
    #[arg(long, required_unless_present = "core", conflicts_with = "core")]
    device: Option<String>,
    #[arg(long, required_unless_present = "device")]
    core: Option<String>,
}

pub fn run() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let bebop = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize()?;
    let repo = paths::buckyball(&bebop);
    let selection = match &cli.command {
        Action::Config(selection) | Action::Build(selection) | Action::Run { selection, .. } => selection,
    };
    let selected = match (&selection.device, &selection.core) {
        (Some(config), None) => format!("device/{config}"),
        (None, Some(core)) => format!("core/{core}"),
        _ => unreachable!(),
    };
    let design = repo.join("examples/designs").join(&selection.design);
    let output = design.join("output/sims").join(&selected);
    let status = match &cli.command {
        Action::Config(_) => Command::new("python3")
            .arg(bebop.join("scripts/config.py"))
            .arg("--buckyball")
            .arg(&repo)
            .arg("--design")
            .arg(&selection.design)
            .arg("--selection")
            .arg(&selected)
            .status()?,
        Action::Build(_) => {
            let manifest = bebop_sim_build::runner_manifest(&output.join("generated/design.pb"))?;
            let target = output.join("emu/bemu/target");
            let status = Command::new("cargo")
                .args([
                    "build",
                    "--release",
                    "--features",
                    "bemu",
                    "--bin",
                    "bebop-bemu",
                    "--manifest-path",
                ])
                .arg(manifest)
                .arg("--target-dir")
                .arg(&target)
                .status()?;
            if status.success() {
                std::fs::create_dir_all(output.join("bin"))?;
                std::fs::copy(target.join("release/bebop-bemu"), output.join("bin/bebop-bemu"))?;
            }
            status
        }
        Action::Run { elf, .. } => Command::new("python3")
            .arg(bebop.join("scripts/run.py"))
            .arg("--buckyball")
            .arg(&repo)
            .arg("--design")
            .arg(&selection.design)
            .arg("--selection")
            .arg(&selected)
            .arg("--elf")
            .arg(elf)
            .status()?,
    };
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
