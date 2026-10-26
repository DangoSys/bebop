use assert_cmd::Command;
use std::path::Path;
use std::time::Duration;

use crate::common::artifacts::ArtifactManager;
use crate::common::discovery::ElfTestCase;
use crate::common::runner::backend::BackendRunner;

#[derive(Clone, Debug, Default)]
pub struct BemuBackend;

impl BackendRunner for BemuBackend {
    fn binary_name(&self) -> &'static str {
        "bebop-bemu"
    }

    fn backend_name(&self) -> &'static str {
        "bemu"
    }

    fn build_command(&self, cmd: &mut Command, bebop_bin: &Path, elf_path: &Path, artifacts: &ArtifactManager) {
        cmd.arg("--elf").arg(elf_path);
        cmd.arg("--log-dir").arg(artifacts.log_dir());
        assert!(
            bebop_bin
                .file_name()
                .expect("runner name")
                .to_string_lossy()
                .eq("bebop-bemu"),
            "BEMU regression requires the design assembly runner"
        );
        cmd.arg("--itrace").arg("--mtrace");
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(1800)
    }

    fn match_case(&self, test_case: &ElfTestCase) -> bool {
        test_case.stem.ends_with("-baremetal")
            || (test_case.stem.starts_with("fw_payload-") && test_case.path.extension().is_some_and(|ext| ext == "elf"))
    }

    fn needs_log_dir(&self) -> bool {
        true
    }
}
