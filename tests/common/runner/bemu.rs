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
        cmd.arg("--elf").arg(elf_path);
        cmd.arg("--log-dir").arg(artifacts.log_dir());
        let stem = elf_path.file_stem().expect("workload file name").to_str().expect("UTF-8 workload name");
        if !whole_chip(stem) {
            assert_eq!(bebop_bin.file_stem().expect("runner name"), "bebop-bemu");
            let (_, core) = bebop_bemu::workload_placement(stem).unwrap_or_else(|error| panic!("{error}"));
            cmd.arg("--core-index").arg(core.to_string());
        }
        cmd.arg("--itrace").arg("--mtrace");
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(1800)
    }

    fn runner_bin(&self, default: &Path, elf_path: &Path) -> PathBuf {
        let stem = elf_path.file_stem().expect("workload file name").to_str().expect("UTF-8 workload name");
        let directory = default.parent().expect("runner directory");
        if !whole_chip(stem) {
            return directory.join("bebop-bemu");
        }
        if default.file_name().expect("runner name").to_string_lossy().starts_with("bebop-chip-") {
            return default.to_path_buf();
        }
        let runners: Vec<_> = std::fs::read_dir(directory).expect("runner directory")
            .map(|entry| entry.expect("runner entry").path())
            .filter(|path| path.is_file() && path.extension().is_none()
                && path.file_name().expect("runner name").to_string_lossy().starts_with("bebop-chip-"))
            .collect();
        let [runner] = runners.as_slice() else {
            panic!("chip workloads require exactly one bebop-chip-* runner in {}", directory.display());
        };
        runner.clone()
    }

    fn match_case(&self, test_case: &ElfTestCase) -> bool {
        test_case.stem.ends_with("-baremetal") ||
            (test_case.stem.starts_with("fw_payload-") && test_case.path.extension().is_some_and(|ext| ext == "elf"))
    }

    fn needs_log_dir(&self) -> bool {
        true
    }
}

fn whole_chip(stem: &str) -> bool {
    stem.starts_with("fw_payload-") || stem.contains("-ctest-chip-") || stem.ends_with("-multicore-baremetal")
}
