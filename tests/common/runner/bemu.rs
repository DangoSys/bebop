use assert_cmd::Command;
use std::path::Path;
use std::time::Duration;

use crate::common::artifacts::ArtifactManager;
use crate::common::discovery::ElfTestCase;
use crate::common::runner::backend::{is_rushb_bemu, is_rushb_verilator, BackendRunner};

#[derive(Clone, Debug, Default)]
pub struct BemuBackend;

impl BackendRunner for BemuBackend {
    fn backend_name(&self) -> &'static str {
        "bemu"
    }

    fn build_command(&self, cmd: &mut Command, bebop_bin: &Path, elf_path: &Path, artifacts: &ArtifactManager) {
        if is_rushb_bemu(elf_path) {
            return;
        }
        if is_rushb_verilator(elf_path) {
            panic!("bemu harness got verilator rushB runner: {}", elf_path.display());
        }

        let direct = bebop_bin.file_stem().is_some_and(|stem| {
            stem == "bebop-bemu" || stem == "bebop_bemu" || stem.to_string_lossy().starts_with("bebop-chip-")
        });
        if direct {
            cmd.arg("--elf").arg(elf_path);
            cmd.arg("--log-dir").arg(artifacts.log_dir());
        } else {
            cmd.arg("run").arg("bemu");
            cmd.arg("--elf").arg(elf_path);
            cmd.arg("--log-dir").arg(artifacts.log_dir());
        }

        if elf_path
            .file_stem()
            .is_some_and(|stem| stem.to_string_lossy().ends_with("-linux"))
        {
            cmd.arg("--pk");
        }
        cmd.arg("--itrace").arg("--mtrace");
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(300)
    }

    fn match_case(&self, test_case: &ElfTestCase) -> bool {
        test_case.stem.ends_with("-rushB-bemu-run")
            || test_case.stem.ends_with("-baremetal")
            || test_case.stem.ends_with("-linux")
    }

    fn needs_log_dir(&self) -> bool {
        true
    }
}
