use std::fs;
use std::path::{Path, PathBuf};

use super::RegressionResult;

const ARTIFACT_ROOT: &str = "test-artifacts";
const DIR_LOG: &str = "log";
const DIR_FST: &str = "fst";
const FILE_STDOUT: &str = "stdout.log";
const FILE_STDERR: &str = "stderr.log";
const FILE_WAVEFORM: &str = "waveform.fst";

fn workspace_root() -> PathBuf {
    std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub struct ArtifactManager {
    root: PathBuf,
}

impl ArtifactManager {
    pub fn clean_all() -> std::io::Result<()> {
        let root = workspace_root().join(ARTIFACT_ROOT);
        if root.exists() {
            fs::remove_dir_all(root)?;
        }
        Ok(())
    }

    /// Create artifact directory with backend and timestamp prefix.
    /// Format: <backend>-<YYYY-MM-DD-HH-MM-SS>-<workload-name>
    pub fn create_with_backend(backend: &str, workload_name: &str) -> std::io::Result<Self> {
        let timestamp = chrono::Local::now().format("%Y-%m-%d-%H-%M-%S");
        let dir_name = format!("{}-{}-{}", backend, timestamp, workload_name);
        let root = workspace_root().join(ARTIFACT_ROOT).join(dir_name);
        fs::create_dir_all(root.join(DIR_LOG))?;
        fs::create_dir_all(root.join(DIR_FST))?;
        Ok(ArtifactManager { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn stdout_path(&self) -> PathBuf {
        self.root.join(FILE_STDOUT)
    }

    pub fn stderr_path(&self) -> PathBuf {
        self.root.join(FILE_STDERR)
    }

    pub fn log_dir(&self) -> PathBuf {
        self.root.join(DIR_LOG)
    }

    pub fn fst_dir(&self) -> PathBuf {
        self.root.join(DIR_FST)
    }

    pub fn fst_waveform_path(&self) -> PathBuf {
        self.fst_dir().join(FILE_WAVEFORM)
    }

    pub fn write_stdout(&self, content: &str) -> std::io::Result<()> {
        fs::write(self.stdout_path(), content)
    }

    pub fn write_stderr(&self, content: &str) -> std::io::Result<()> {
        fs::write(self.stderr_path(), content)
    }

    pub fn write_summary(&self, result: &RegressionResult) -> std::io::Result<()> {
        result.write_summary(&self.root)
    }

    pub fn finalize(self, _test_passed: bool) -> Option<PathBuf> {
        // Always keep artifacts from the current run for inspection.
        Some(self.root.clone())
    }
}

impl std::fmt::Debug for ArtifactManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArtifactManager").field("root", &self.root).finish()
    }
}
