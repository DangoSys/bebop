use clap::{Args, Parser, Subcommand};
use std::{error::Error, ffi::OsString, path::PathBuf, process::Command};

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
        /// Model runner options, such as --initrd or --ddr-image, after --.
        #[arg(last = true)]
        runner_args: Vec<OsString>,
    },
}

#[derive(Args)]
struct Selection {
    /// Config TOML Top relative to examples/designs.
    #[arg(long)]
    config: PathBuf,
    #[arg(long, required_unless_present = "hart", conflicts_with = "hart")]
    device: Option<String>,
    #[arg(long, required_unless_present = "device")]
    hart: Option<String>,
}

pub fn run() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let bebop = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize()?;
    let repo = paths::buckyball(&bebop);
    let selection = match &cli.command {
        Action::Config(selection) | Action::Build(selection) | Action::Run { selection, .. } => selection,
    };
    let (kind, name) = match (&selection.device, &selection.hart) {
        (Some(device), None) => ("device", device),
        (None, Some(hart)) => ("hart", hart),
        _ => unreachable!(),
    };
    let stem = selection.config.file_stem().unwrap().to_str().unwrap();
    let selected = format!("{kind}/{name}/{stem}");
    let design_name = selection.config.iter().next().unwrap().to_str().unwrap();
    let design = repo.join("examples/designs").join(design_name);
    let output = design.join("output/sims").join(&selected);
    let status = match &cli.command {
        Action::Config(_) => Command::new("python3")
            .arg(bebop.join("scripts/config.py"))
            .arg("--buckyball")
            .arg(&repo)
            .arg("--config")
            .arg(&selection.config)
            .arg("--kind")
            .arg(kind)
            .arg("--name")
            .arg(name)
            .status()?,
        Action::Build(_) => {
            let manifest = bebop_sim_build::runner_manifest(&output.join("generated/design.pb"))?;
            let target = output.join("emu/bemu/target");
            let status = Command::new("cargo")
                .args([
                    "build",
                    "--locked",
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
        Action::Run { elf, runner_args, .. } => Command::new("python3")
            .arg(bebop.join("scripts/run.py"))
            .arg("--buckyball")
            .arg(&repo)
            .arg("--design")
            .arg(design_name)
            .arg("--selection")
            .arg(&selected)
            .arg("--elf")
            .arg(elf)
            .arg("--")
            .args(runner_args)
            .status()?,
    };
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
