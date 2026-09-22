use assert_cmd::Command;
use std::path::Path;
use std::time::Duration;

use crate::common::artifacts::ArtifactManager;
use crate::common::discovery::ElfTestCase;
use crate::common::runner::backend::{is_rushb_bemu, is_rushb_verilator, BackendRunner};

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
        if is_rushb_verilator(elf_path) {
            return;
        }
        if is_rushb_bemu(elf_path) {
            panic!("verilator harness got bemu rushB runner: {}", elf_path.display());
        }

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
        test_case.stem.ends_with("-rushB-verilator-run")
            || test_case.stem.ends_with("-baremetal")
            || test_case.stem.ends_with("-linux")
    }

    fn configure_command_env(&self, cmd: &mut Command, elf_path: &Path) {
        if !is_rushb_verilator(elf_path) {
            cmd.env(
                "ARCH_CONFIG",
                self.arch_config
                    .as_deref()
                    .unwrap_or("sims.verilator.BuckyballToyVerilatorConfig"),
            );
        }
    }

    fn configure_command_dir(&self, cmd: &mut Command, elf_path: &Path) {
        if is_rushb_verilator(elf_path) {
            if let Some(dir) = elf_path.parent() {
                cmd.current_dir(dir);
            }
        } else if self.diff {
            cmd.current_dir(env!("CARGO_MANIFEST_DIR"));
        }
    }

    fn needs_log_dir(&self) -> bool {
        true
    }
}
