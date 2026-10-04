use assert_cmd::Command;
use std::path::Path;
use std::time::Duration;

use crate::common::artifacts::ArtifactManager;
use crate::common::discovery::ElfTestCase;
use crate::common::runner::backend::BackendRunner;

#[derive(Clone, Debug, Default)]
pub struct VerilatorBackend {
    diff: bool,
    arch_config: Option<String>,
}

impl VerilatorBackend {
    pub fn new(diff: bool, arch_config: Option<String>) -> Self {
        Self { diff, arch_config }
    }
}

impl BackendRunner for VerilatorBackend {
    fn backend_name(&self) -> &'static str {
        if self.diff {
            "difftest"
        } else {
            "verilator"
        }
    }

    fn verbose_run_kind(&self) -> &'static str {
        "verilator test"
    }

    fn build_command(&self, cmd: &mut Command, _bebop_bin: &Path, elf_path: &Path, artifacts: &ArtifactManager) {
        cmd.arg("run").arg("verilator");
        cmd.arg("--elf").arg(elf_path);
        cmd.arg("--log-dir").arg(artifacts.log_dir());
        cmd.arg("--no-wave").arg("--itrace").arg("--mtrace");
        if self.diff {
            cmd.arg("--diff");
        }
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(1800)
    }

    fn match_case(&self, test_case: &ElfTestCase) -> bool {
        test_case.stem.ends_with("-baremetal") || test_case.stem.ends_with("-linux")
    }

    fn configure_command_env(&self, cmd: &mut Command, _elf_path: &Path) {
        cmd.env(
            "ARCH_CONFIG",
            self.arch_config
                .as_deref()
                .expect("--arch-config is required for Verilator regression"),
        );
    }

    fn configure_command_dir(&self, cmd: &mut Command, _elf_path: &Path) {
        if self.diff {
            cmd.current_dir(env!("CARGO_MANIFEST_DIR"));
        }
    }

    fn needs_log_dir(&self) -> bool {
        true
    }
}
