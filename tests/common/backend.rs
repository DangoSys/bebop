use assert_cmd::Command;
use std::path::Path;
use std::time::Duration;

use super::artifact::ArtifactManager;

pub trait BackendRunner {
    fn backend_name(&self) -> &'static str;

    fn verbose_run_kind(&self) -> &'static str {
        "test"
    }

    fn timeout(&self) -> Duration;

    fn configure_command_env(&self, _cmd: &mut Command) {}

    fn build_command(
        &self,
        cmd: &mut Command,
        bebop_bin: &Path,
        elf_path: &Path,
        artifacts: &ArtifactManager,
    );

    fn needs_log_dir(&self) -> bool {
        false
    }

    fn needs_fst_dir(&self) -> bool {
        false
    }
}

#[cfg(feature = "bemu")]
#[derive(Clone, Copy, Debug, Default)]
pub struct BemuBackend;

#[cfg(feature = "bemu")]
impl BackendRunner for BemuBackend {
    fn backend_name(&self) -> &'static str {
        "bemu"
    }

    fn build_command(
        &self,
        cmd: &mut Command,
        _bebop_bin: &Path,
        elf_path: &Path,
        artifacts: &ArtifactManager,
    ) {
        cmd.arg("bemu");
        cmd.arg("--elf").arg(elf_path);
        cmd.arg("--log-dir").arg(artifacts.root());
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(300)
    }

    fn needs_log_dir(&self) -> bool {
        true
    }
}

#[cfg(feature = "verilator")]
#[derive(Clone, Copy, Debug, Default)]
pub struct VerilatorBackend;

#[cfg(feature = "verilator")]
impl BackendRunner for VerilatorBackend {
    fn backend_name(&self) -> &'static str {
        "verilator"
    }

    fn verbose_run_kind(&self) -> &'static str {
        "verilator test"
    }

    fn build_command(
        &self,
        cmd: &mut Command,
        _bebop_bin: &Path,
        elf_path: &Path,
        artifacts: &ArtifactManager,
    ) {
        cmd.arg("verilator");
        cmd.arg("--elf").arg(elf_path);
        cmd.arg("--log-dir").arg(artifacts.log_dir());
        cmd.arg("--fst-dir").arg(artifacts.fst_dir());
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(600)
    }

    fn configure_command_env(&self, cmd: &mut Command) {
        cmd.env(
            "ARCH_CONFIG",
            "sims.verilator.BuckyballToyVerilatorConfig",
        );
    }

    fn needs_log_dir(&self) -> bool {
        true
    }

    fn needs_fst_dir(&self) -> bool {
        true
    }
}
