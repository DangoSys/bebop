use assert_cmd::Command;
use std::path::Path;
use std::time::Duration;

use super::super::artifacts::ArtifactManager;
use super::super::discovery::ElfTestCase;

pub trait BackendRunner {
    fn backend_name(&self) -> &'static str;

    fn verbose_run_kind(&self) -> &'static str {
        "test"
    }

    fn timeout(&self) -> Duration;

    fn configure_command_env(&self, _cmd: &mut Command, _elf_path: &Path) {}

    fn build_command(&self, cmd: &mut Command, bebop_bin: &Path, elf_path: &Path, artifacts: &ArtifactManager);

    fn configure_command_dir(&self, _cmd: &mut Command, _elf_path: &Path) {}

    fn scan_extension(&self) -> Option<&'static str> {
        None
    }

    fn match_case(&self, test_case: &ElfTestCase) -> bool;

    fn needs_log_dir(&self) -> bool {
        false
    }

    fn needs_wave(&self) -> bool {
        false
    }
}
