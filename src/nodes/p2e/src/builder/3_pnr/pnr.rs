use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[path = "artifacts.rs"]
mod artifacts;

pub struct PnrStep {
    pub output_dir: PathBuf,
}

impl PnrStep {
    pub fn new(output_dir: PathBuf) -> Self {
        Self { output_dir }
    }

    fn command(&self, script: &str) -> Command {
        let mut command = Command::new("bash");
        command
            .args(["-ec", script, "p2e-build"])
            .arg(&self.output_dir)
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("sourceme.sh"));
        command
    }

    fn make(&self, targets: &[&str]) -> Result<(), String> {
        let status = self
            .command(
                "cd \"$1\"; source \"$2\"; shift 2; exec make -C fpgaCompDir SHELL=/bin/bash '.SHELLFLAGS=-e -o pipefail -c' \"$@\"",
            )
            .args(targets)
            .status()
            .map_err(|e| format!("PNR make {targets:?}: {e}"))?;
        if !status.success() {
            return Err(format!("PNR make {targets:?} failed: {status}"));
        }
        Ok(())
    }

    fn partitions(&self, parts: &[String], target: &str) -> Result<(), String> {
        let mut commands = Vec::new();
        for part in parts {
            let log = std::fs::File::create(
                self.output_dir
                    .join("fpgaCompDir")
                    .join(part)
                    .join(format!(".{target}.log")),
            )
            .map_err(|e| format!("{target} log for {part}: {e}"))?;
            let stderr = log.try_clone().map_err(|e| e.to_string())?;
            let mut command = if target == "syn" || target == "pnr" {
                let mut command = self.command("cd \"$1\"; source \"$2\"; exec make -C fpgaCompDir SHELL=/bin/bash '.SHELLFLAGS=-e -o pipefail -c' \"$3\"");
                command.arg(format!("{target}_{part}"));
                command
            } else {
                let mut command = self.command("cd \"$1\"; source \"$2\"; exec make -C fpgaCompDir -f \"$3/Makefile\" SHELL=/bin/bash '.SHELLFLAGS=-e -o pipefail -c' \"$4\"");
                command.arg(part).arg(target);
                command
            };
            command.stdout(Stdio::from(log)).stderr(Stdio::from(stderr));
            commands.push((part, command));
        }
        let mut jobs = Vec::new();
        let mut errors = Vec::new();
        for (part, mut command) in commands {
            match command.spawn() {
                Ok(child) => jobs.push((part, child)),
                Err(e) => {
                    errors.push(format!("launch {part}: {e}"));
                    break;
                }
            }
        }
        for (part, mut child) in jobs {
            match child.wait() {
                Ok(status) if status.success() => {}
                Ok(status) => errors.push(format!("{part}: {status}")),
                Err(e) => errors.push(format!("wait {part}: {e}")),
            }
        }
        if !errors.is_empty() {
            return Err(format!("{target} failed: {}", errors.join("; ")));
        }
        Ok(())
    }

    fn dbg_gen(&self, step: &str) -> Result<(), String> {
        let mut command = self.command("cd \"$1\"; source \"$2\"; export WORK_PATH=\"$PWD\" MEMORYFILEPATH=\"$PWD/\"; shift 2; if [[ \"$1\" == -step1 ]]; then cd fpgaCompDir; fi; exec \"$VDBG_HOME/bin/dbgGen\" \"$WORK_PATH\" \"$@\"");
        command.arg(step);
        if step == "-step1" {
            command.arg("-log_overwrite");
        }
        let status = command.status().map_err(|e| format!("dbgGen launch: {e}"))?;
        if !status.success() {
            return Err(format!("dbgGen {step} failed: {status}"));
        }
        Ok(())
    }

    pub fn run(&self) -> Result<PathBuf, String> {
        let parts = artifacts::partitions(&self.output_dir)?;
        let sta_command = artifacts::sta_command(&self.output_dir)?;
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/builder/3_pnr/PNR_settings.tcl"),
            self.output_dir.join("PNR_settings.tcl"),
        )
        .map_err(|e| format!("PNR settings: {e}"))?;
        self.make(&["clean"])?;
        self.partitions(&parts, "syn")?;
        self.partitions(&parts, "pnr")?;
        let primary = artifacts::bitstreams(&self.output_dir, &parts)?;
        self.make(&["post_pnr_summary"])?;
        // Execute the generated shared timing command once, then join every refresh job.
        let status = self
            .command("cd \"$1\"; source \"$2\"; export WORK_PATH=\"$PWD\"; cd fpgaCompDir; exec bash -e -o pipefail -c \"$3\"")
            .arg(sta_command)
            .status()
            .map_err(|e| format!("postPrTiming launch: {e}"))?;
        if !status.success() {
            return Err(format!("postPrTiming failed: {status}"));
        }
        self.partitions(&parts, "reg_init_refresh")?;
        self.make(&["readbackDB_create"])?;
        self.dbg_gen("-step1")?;
        self.partitions(&parts, "find_revise_net_name")?;
        self.dbg_gen("-step2")?;
        artifacts::runtime(&self.output_dir)?;
        let bitstream = self.output_dir.join("fpgaCompDir/bitstream.bit");
        std::fs::copy(primary, &bitstream).map_err(|e| format!("Publish primary bitstream: {e}"))?;
        Ok(bitstream)
    }
}
