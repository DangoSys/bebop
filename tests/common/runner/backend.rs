use assert_cmd::Command;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::super::artifacts::ArtifactManager;
use super::super::discovery::ElfTestCase;

pub(crate) fn path_stem(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

pub(crate) fn is_rushb_bemu(path: &Path) -> bool {
    path_stem(path).is_some_and(|n| n.ends_with("-rushB-bemu-run"))
}

pub(crate) fn is_rushb_verilator(path: &Path) -> bool {
    path_stem(path).is_some_and(|n| n.ends_with("-rushB-verilator-run"))
}

pub trait BackendRunner {
    fn backend_name(&self) -> &'static str;

    fn verbose_run_kind(&self) -> &'static str {
        "test"
    }

    fn timeout(&self) -> Duration;

    fn configure_command_env(&self, _cmd: &mut Command, _elf_path: &Path) {}

    fn build_command(&self, cmd: &mut Command, bebop_bin: &Path, elf_path: &Path, artifacts: &ArtifactManager);

    /// Guest backends use bebop; rushB host runners are executed directly.
    fn command_program(&self, bebop_bin: &Path, elf_path: &Path) -> PathBuf {
        if is_rushb_bemu(elf_path) || is_rushb_verilator(elf_path) {
            elf_path.to_path_buf()
        } else {
            bebop_bin.to_path_buf()
        }
    }

    fn configure_command_dir(&self, cmd: &mut Command, elf_path: &Path) {
        if is_rushb_bemu(elf_path) || is_rushb_verilator(elf_path) {
            if let Some(dir) = elf_path.parent() {
                cmd.current_dir(dir);
            }
        }
    }

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
