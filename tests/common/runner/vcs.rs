use assert_cmd::Command;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::common::artifacts::ArtifactManager;
use crate::common::discovery::ElfTestCase;
use crate::common::runner::backend::BackendRunner;

#[derive(Clone)]
pub struct VcsBackend {
    pub simv: PathBuf,
    pub dram_configs: PathBuf,
}

impl BackendRunner for VcsBackend {
    fn binary_name(&self) -> &'static str {
        "bebop-vcs"
    }
    fn backend_name(&self) -> &'static str {
        "vcs"
    }
    fn timeout(&self) -> Duration {
        Duration::from_secs(8 * 60 * 60)
    }
    fn needs_log_dir(&self) -> bool {
        true
    }
    fn match_case(&self, _case: &ElfTestCase) -> bool {
        true
    }
    fn build_command(&self, command: &mut Command, _: &Path, elf: &Path, artifacts: &ArtifactManager) {
        command
            .arg("--simv")
            .arg(&self.simv)
            .arg("--dram-configs")
            .arg(&self.dram_configs)
            .arg("--elf")
            .arg(elf)
            .arg("--log-dir")
            .arg(artifacts.log_dir());
    }
}
