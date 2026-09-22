use assert_cmd::Command;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::common::artifacts::ArtifactManager;
use crate::common::discovery::ElfTestCase;
use crate::common::runner::backend::{is_rushb_bemu, is_rushb_verilator, BackendRunner};

#[derive(Clone, Debug)]
pub struct P2eBackend {
    bitstream: PathBuf,
    diff: bool,
}

impl P2eBackend {
    pub fn new(bitstream: PathBuf, diff: bool) -> Self {
        Self { bitstream, diff }
    }
}

impl BackendRunner for P2eBackend {
    fn backend_name(&self) -> &'static str {
        if self.diff {
            "p2e-difftest"
        } else {
            "p2e"
        }
    }

    fn verbose_run_kind(&self) -> &'static str {
        "p2e test"
    }

    fn build_command(&self, cmd: &mut Command, _bebop_bin: &Path, elf_path: &Path, artifacts: &ArtifactManager) {
        if is_rushb_bemu(elf_path) || is_rushb_verilator(elf_path) {
            panic!("p2e does not support rushB runners: {}", elf_path.display());
        }
        cmd.arg("run").arg("p2e");
        cmd.arg("--image").arg(elf_path);
        cmd.arg("--bitstream").arg(&self.bitstream);
        cmd.arg("--log-dir").arg(artifacts.log_dir());
        if self.diff {
            let reference = if elf_path
                .file_stem()
                .is_some_and(|stem| stem.to_string_lossy().ends_with("-pk"))
            {
                elf_path.with_extension("elf")
            } else {
                elf_path.with_extension("")
            };
            assert!(
                reference.is_file(),
                "P2E DiffTest workload ELF not found: {}",
                reference.display()
            );
            cmd.arg("--diff").arg("--image-elf").arg(reference);
        }
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(1800)
    }

    fn scan_extension(&self) -> Option<&'static str> {
        Some("hex")
    }

    fn match_case(&self, test_case: &ElfTestCase) -> bool {
        test_case.stem.ends_with("-baremetal")
            || (test_case.stem.starts_with("fw_payload-") && test_case.stem.ends_with("-pk"))
    }

    fn needs_log_dir(&self) -> bool {
        true
    }
}
