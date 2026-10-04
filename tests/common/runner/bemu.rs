use assert_cmd::Command;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::common::artifacts::ArtifactManager;
use crate::common::discovery::ElfTestCase;
use crate::common::runner::backend::BackendRunner;

#[derive(Clone, Debug, Default)]
pub struct BemuBackend;

impl BackendRunner for BemuBackend {
    fn backend_name(&self) -> &'static str {
        "bemu"
    }

    fn build_command(&self, cmd: &mut Command, bebop_bin: &Path, elf_path: &Path, artifacts: &ArtifactManager) {
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
        let stem = elf_path.file_stem().expect("workload file name").to_str().expect("UTF-8 workload name");
        let (tile, core) = bebop_bemu::workload_placement(stem).unwrap_or_else(|error| panic!("{error}"));
        if stem.ends_with("-linux") {
            cmd.arg("--core-index").arg(core.to_string());
        } else {
            cmd.arg("--tile-index").arg(tile.to_string());
        }
        cmd.arg("--itrace").arg("--mtrace");
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(300)
    }

    /// Baremetal workloads run on the chip's tile runner; a pk (Linux user-mode) process runs on
    /// the single-core bebop-bemu built beside it.
    fn runner_bin(&self, default: &Path, elf_path: &Path) -> PathBuf {
        let linux = elf_path.file_stem().is_some_and(|stem| stem.to_string_lossy().ends_with("-linux"));
        let core = default.with_file_name("bebop-bemu");
        if linux && core.is_file() { core } else { default.to_path_buf() }
    }

    fn match_case(&self, test_case: &ElfTestCase) -> bool {
        test_case.stem.ends_with("-baremetal") || test_case.stem.ends_with("-linux")
    }

    fn needs_log_dir(&self) -> bool {
        true
    }
}
